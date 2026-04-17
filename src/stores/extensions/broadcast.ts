/**
 * Extension Broadcast Store
 *
 * Every registered iframe gets its own `MessageChannel` — the main window
 * keeps `port1`, the extension's SDK receives `port2` during the PORT_INIT
 * handshake. After that, all bidirectional traffic (host → iframe events,
 * iframe → host requests) flows through the private port and never touches
 * the window-level `postMessage` channel. The port is the trust anchor:
 * whoever holds its pair is authentic by construction.
 *
 * Event categories (unchanged) — but routing is now permission-scoped:
 *   - Context Changed: all extensions (public metadata).
 *   - Sync Tables Updated: filtered by Rust `extension_filter_sync_tables`.
 *   - File Changed: filtered by Rust-computed `readerExtensionIds`.
 *   - Shell output / exit: scoped to the session's owning extension.
 *   - External request: routed to the target extension only.
 *
 * Startup buffering: events that arrive before the SDK finishes its handshake
 * are buffered per-iframe and flushed on PORT_READY — no events are dropped
 * during the startup window.
 */

import { invoke } from '@tauri-apps/api/core'
import {
  TAURI_COMMANDS,
  HAEXTENSION_EVENTS,
  EXTERNAL_EVENTS,
  SHELL_EVENTS,
  HAEXSPACE_MESSAGE_TYPES,
  type ApplicationContext,
  type FileChangePayload,
  type ExternalRequestPayload,
  type FilteredSyncTablesResult,
} from '@haex-space/vault-sdk'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import { createLogger } from '~/stores/logging'
import {
  dispatchFileChangedBroadcast,
  dispatchShellEventBroadcast,
} from './broadcastRouting'
import {
  handleExtensionRequestAsync,
  type ExtensionResponse,
} from '~/composables/extensionMessageHandler'
import type { ExtensionRequest } from '~/composables/handlers/types'

const log = createLogger('BROADCAST')

/**
 * Per-iframe state. The port lives as long as the iframe is registered;
 * closing the port (on unregister) disconnects both sides atomically.
 *
 * `ready` flips to `true` after PORT_READY arrives on port1. While `false`,
 * broadcasts are collected in `buffer` and flushed at the moment of ACK.
 */
interface ExtensionIframeEntry {
  extension: IHaexSpaceExtension
  windowId: string
  port: MessagePort
  ready: boolean
  buffer: Array<Record<string, unknown>>
}

/** Public shape — omits the port internals from consumers of the store. */
interface ExtensionInstance {
  extension: IHaexSpaceExtension
  windowId: string
}

export const useExtensionBroadcastStore = defineStore('extensionBroadcastStore', () => {
  const deviceStore = useDeviceStore()
  const { isDesktop } = storeToRefs(deviceStore)

  // Map iframe element to entry. Use markRaw to prevent Vue reactivity from
  // trying to proxy DOM elements / MessagePorts.
  const iframeRegistry = markRaw(new Map<HTMLIFrameElement, ExtensionIframeEntry>())

  /**
   * Register an iframe for MessagePort-based communication.
   *
   * 1. Create a MessageChannel.
   * 2. Listen on port1 — receives PORT_READY (ACK) and subsequent extension
   *    requests.
   * 3. Send port2 to the iframe once its document has loaded; the SDK's
   *    PORT_INIT handler grabs it, attaches its own listener, and sends
   *    PORT_READY back on port1.
   */
  const registerIframe = (
    iframe: HTMLIFrameElement,
    extension: IHaexSpaceExtension,
    windowId: string,
  ) => {
    const channel = new MessageChannel()
    const entry: ExtensionIframeEntry = {
      extension,
      windowId,
      port: channel.port1,
      ready: false,
      buffer: [],
    }
    iframeRegistry.set(iframe, entry)

    channel.port1.addEventListener('message', (event) => {
      handlePortMessageAsync(entry, event).catch((err) => {
        log.error('Failed to handle port message:', err)
      })
    })
    channel.port1.start()

    // Transfer port2 to the iframe. We do this once the iframe's document has
    // loaded so the SDK's handshake listener is guaranteed to be installed.
    const sendPort = () => {
      if (!iframe.contentWindow) return
      try {
        iframe.contentWindow.postMessage(
          { type: HAEXSPACE_MESSAGE_TYPES.PORT_INIT },
          '*',
          [channel.port2],
        )
      }
      catch (err) {
        log.error(`Failed to send PORT_INIT to extension ${extension.name}:`, err)
      }
    }

    // Best-effort detection: if the iframe is already loaded (readyState
    // complete), send immediately; otherwise wait for the load event.
    const alreadyLoaded
      = iframe.contentDocument?.readyState === 'complete'
      || iframe.contentDocument?.readyState === 'interactive'
    if (alreadyLoaded) {
      sendPort()
    }
    else {
      iframe.addEventListener('load', sendPort, { once: true })
    }

    log.info(`Registered iframe for ${extension.name} (windowId: ${windowId})`)
  }

  /**
   * Unregister an iframe. Closes its port — the extension's SDK observes
   * the port as severed and the browser GCs both ends.
   */
  const unregisterIframe = (iframe: HTMLIFrameElement) => {
    const entry = iframeRegistry.get(iframe)
    if (!entry) return
    try {
      entry.port.close()
    }
    catch {
      // Already closed — ignore.
    }
    entry.buffer.length = 0
    iframeRegistry.delete(iframe)
    log.info(`Unregistered iframe for ${entry.extension.name}`)
  }

  /**
   * Handle a message arriving on port1 from the extension's port2.
   * Two categories:
   *   - PORT_READY: handshake ACK. Mark entry ready, flush buffered events.
   *   - Anything else: extension request. Route to the handler and post
   *     the response back on the same port.
   */
  const handlePortMessageAsync = async (
    entry: ExtensionIframeEntry,
    event: MessageEvent,
  ): Promise<void> => {
    const data = event.data as { type?: string } | null

    if (data?.type === HAEXSPACE_MESSAGE_TYPES.PORT_READY) {
      if (entry.ready) return // Ignore duplicate ACKs
      entry.ready = true
      log.info(`Port READY for ${entry.extension.name} (flushing ${entry.buffer.length} buffered events)`)
      for (const bufferedMessage of entry.buffer) {
        entry.port.postMessage(bufferedMessage)
      }
      entry.buffer.length = 0
      return
    }

    // Extension request (method + id + params + timestamp).
    const request = event.data as ExtensionRequest
    const instance: ExtensionInstance = {
      extension: entry.extension,
      windowId: entry.windowId,
    }
    const response: ExtensionResponse = await handleExtensionRequestAsync(request, instance)
    entry.port.postMessage(response)
  }

  /**
   * Iterate entries — used by dispatchers and test helpers.
   * Each call yields `{instance, port, ready, buffer}` pairs in registration order.
   */
  const entriesForDispatch = () => {
    const out: Array<{
      instance: { extension: { id: string } }
      port: MessagePort
      ready: boolean
      buffer: Array<Record<string, unknown>>
    }> = []
    for (const entry of iframeRegistry.values()) {
      out.push({
        instance: { extension: { id: entry.extension.id } },
        port: entry.port,
        ready: entry.ready,
        buffer: entry.buffer,
      })
    }
    return out
  }

  /**
   * Get all entries for a specific extension (multi-instance support).
   * Preserves the legacy shape used by external callers.
   */
  const getAllWindowsForExtension = (
    extensionId: string,
  ): Array<{ instance: ExtensionInstance; port: MessagePort }> => {
    const result: Array<{ instance: ExtensionInstance; port: MessagePort }> = []
    for (const entry of iframeRegistry.values()) {
      if (entry.extension.id === extensionId) {
        result.push({
          instance: { extension: entry.extension, windowId: entry.windowId },
          port: entry.port,
        })
      }
    }
    return result
  }

  // ============================================================================
  // Broadcasting
  // ============================================================================

  /**
   * Broadcast a context change. Public metadata — sent to every extension.
   * Not-yet-ready iframes buffer the event and receive it after PORT_READY.
   */
  const broadcastContext = async (context: ApplicationContext) => {
    const message = {
      type: HAEXTENSION_EVENTS.CONTEXT_CHANGED,
      data: { context },
      timestamp: Date.now(),
    }

    for (const entry of iframeRegistry.values()) {
      if (entry.ready) entry.port.postMessage(message)
      else entry.buffer.push(message)
    }

    // Webview-mode extensions still use Tauri emit — unaffected by the port
    // handshake since they don't run in an iframe.
    if (isDesktop.value) {
      try {
        await invoke(TAURI_COMMANDS.extension.webviewBroadcast, {
          event: HAEXTENSION_EVENTS.CONTEXT_CHANGED,
          payload: { context },
        })
      }
      catch (error) {
        log.error('Failed to broadcast to webview extensions:', error)
      }
    }
  }

  /**
   * Broadcast filtered sync:tables-updated events. Each extension receives
   * only the table names they are authorised for; the authorisation list is
   * computed in Rust via `extension_filter_sync_tables`.
   */
  const broadcastSyncTablesUpdated = async (tables: string[]) => {
    if (tables.length === 0) return

    const result = await invoke<FilteredSyncTablesResult>(
      TAURI_COMMANDS.extension.filterSyncTables,
      { tables },
    )

    for (const entry of iframeRegistry.values()) {
      const allowedTables = result.extensions[entry.extension.id]
      if (!allowedTables || allowedTables.length === 0) continue

      const message = {
        type: HAEXTENSION_EVENTS.SYNC_TABLES_UPDATED,
        data: { tables: allowedTables },
        timestamp: Date.now(),
      }
      if (entry.ready) entry.port.postMessage(message)
      else entry.buffer.push(message)
    }

    if (isDesktop.value) {
      try {
        await invoke(TAURI_COMMANDS.extension.emitSyncTables, {
          filteredExtensions: result,
        })
      }
      catch (error) {
        log.error('Failed to emit to webview extensions:', error)
      }
    }
  }

  const broadcastFileChanged = (
    payload: FileChangePayload & { readerExtensionIds?: string[] },
  ) => {
    dispatchFileChangedBroadcast(payload, entriesForDispatch())
  }

  const broadcastShellEvent = (
    type: string,
    payload: Record<string, unknown> & { extensionId: string },
  ) => {
    dispatchShellEventBroadcast(type, payload, entriesForDispatch())
  }

  /**
   * Forward an external request to the extension it targets (first matching
   * iframe). External requests expect a single response, so we fan out to
   * exactly one instance.
   */
  const forwardExternalRequest = (payload: ExternalRequestPayload) => {
    const { extensionPublicKey, extensionName } = payload

    for (const entry of iframeRegistry.values()) {
      if (
        entry.extension.publicKey === extensionPublicKey
        && entry.extension.name === extensionName
      ) {
        const message = {
          type: EXTERNAL_EVENTS.REQUEST,
          data: {
            requestId: payload.requestId,
            publicKey: payload.publicKey,
            action: payload.action,
            payload: payload.payload,
          },
          timestamp: Date.now(),
        }
        if (entry.ready) entry.port.postMessage(message)
        else entry.buffer.push(message)
        log.info(`Forwarded external request to: ${entry.extension.name}`)
        return
      }
    }

    log.warn(`No registered iframe for external request: ${extensionName}`)
  }

  // ============================================================================
  // Tauri event listeners
  // ============================================================================

  let eventListenersRegistered = false
  const unlistenFns: UnlistenFn[] = []

  const setupEventListeners = async () => {
    if (eventListenersRegistered) return
    eventListenersRegistered = true

    try {
      unlistenFns.push(
        await listen<ExternalRequestPayload>(EXTERNAL_EVENTS.REQUEST, (event) => {
          forwardExternalRequest(event.payload)
        }),
      )

      unlistenFns.push(
        await listen<FileChangePayload & { readerExtensionIds: string[] }>(
          HAEXTENSION_EVENTS.FILE_CHANGED,
          (event) => {
            broadcastFileChanged(event.payload)
          },
        ),
      )

      unlistenFns.push(
        await listen<{ sessionId: string; extensionId: string; data: string }>(
          SHELL_EVENTS.OUTPUT,
          (event) => {
            broadcastShellEvent(SHELL_EVENTS.OUTPUT, event.payload)
          },
        ),
      )
      unlistenFns.push(
        await listen<{ sessionId: string; extensionId: string; exitCode: number | null }>(
          SHELL_EVENTS.EXIT,
          (event) => {
            broadcastShellEvent(SHELL_EVENTS.EXIT, event.payload)
          },
        ),
      )
    }
    catch (error) {
      log.error('Failed to setup event listeners:', error)
    }
  }

  const cleanup = () => {
    for (const unlisten of unlistenFns) unlisten()
    unlistenFns.length = 0
    eventListenersRegistered = false

    for (const entry of iframeRegistry.values()) {
      try {
        entry.port.close()
      }
      catch {
        // ignore
      }
    }
    iframeRegistry.clear()
  }

  return {
    registerIframe,
    unregisterIframe,
    getAllWindowsForExtension,
    iframeRegistry,

    broadcastContext,
    broadcastSyncTablesUpdated,
    broadcastFileChanged,
    broadcastShellEvent,
    forwardExternalRequest,

    setupEventListeners,
    cleanup,
  }
})
