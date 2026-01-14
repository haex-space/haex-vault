/**
 * Extension Broadcast Store
 *
 * Centralized store for broadcasting events to extensions.
 * Handles both iframe (postMessage) and webview (Tauri emit) modes.
 *
 * Broadcasting architecture:
 * - Frontend-centralized: All logic in this store, Rust only provides filtering and targeted emit
 * - Events go to ALL instances of an extension (extensions handle deduplication if needed)
 * - Permission filtering by HOST (Rust filters, extensions do NOT filter - security requirement)
 *
 * Event categories:
 * - Context Changed: Public, sent to all extensions
 * - Sync Tables Updated: Filtered by database permissions
 * - File Changed: Filtered by filesystem permissions
 * - External Request: Sent to specific extension only (first instance)
 * - LocalSend Events: Filtered by localsend permission
 *
 * Extension identification:
 * - Desktop: Origin contains base64-encoded extension info (haex-extension://<base64>)
 * - Android: Origin is http://haex-extension.localhost, need contentWindow matching
 *
 * NOTE: This store only BROADCASTS. It does NOT store context or any other state.
 */

import { invoke } from '@tauri-apps/api/core'
import {
  TAURI_COMMANDS,
  HAEXTENSION_EVENTS,
  EXTERNAL_EVENTS,
  LOCALSEND_EVENTS,
  type ApplicationContext,
  type FileChangePayload,
  type ExternalRequestPayload,
  type FilteredSyncTablesResult,
} from '@haex-space/vault-sdk'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import { createLogger } from '~/stores/logging'

const log = createLogger('BROADCAST')

// Extension instance info for iframe registry
interface ExtensionInstance {
  extension: IHaexSpaceExtension
  windowId: string
}

export const useExtensionBroadcastStore = defineStore('extensionBroadcastStore', () => {
  const deviceStore = useDeviceStore()
  const { isDesktop } = storeToRefs(deviceStore)

  // ============================================================================
  // Iframe Registry (for postMessage communication)
  // ============================================================================

  // Map iframe element to extension instance
  // Use markRaw to prevent Vue reactivity from trying to proxy DOM elements
  const iframeRegistry = markRaw(new Map<HTMLIFrameElement, ExtensionInstance>())

  // Cache: event.source Window -> extension instance (performance optimization)
  // On Android, origin doesn't contain extension info, so we use contentWindow
  // matching on first message and cache the result for faster subsequent lookups.
  // IMPORTANT: Use markRaw to prevent Vue from trying to access properties on
  // cross-origin Window objects (which would throw SecurityError)
  const sourceCache = markRaw(new Map<Window, ExtensionInstance>())

  /**
   * Register an iframe for message handling.
   * Context will be sent when the iframe requests it via extension_context_get.
   */
  const registerIframe = (
    iframe: HTMLIFrameElement,
    extension: IHaexSpaceExtension,
    windowId: string,
  ) => {
    log.info(`========== REGISTERING IFRAME ==========`)
    log.info(`Extension: ${extension.name} (ID: ${extension.id})`)
    log.info(`Window ID: ${windowId}`)
    log.debug('Extension publicKey:', extension.publicKey)
    log.debug('Has contentWindow:', !!iframe.contentWindow)
    log.debug('Iframe connected:', iframe.isConnected)
    log.debug('Current registry size:', iframeRegistry.size)
    iframeRegistry.set(iframe, { extension, windowId })
    log.info(`Iframe registered - new registry size: ${iframeRegistry.size}`)
  }

  /**
   * Unregister an iframe
   */
  const unregisterIframe = (iframe: HTMLIFrameElement) => {
    const instance = iframeRegistry.get(iframe)
    log.info(`========== UNREGISTERING IFRAME ==========`)
    log.debug('Has instance:', !!instance)
    log.debug('Extension name:', instance?.extension.name)
    log.debug('Window ID:', instance?.windowId)
    log.debug('Current registry size:', iframeRegistry.size)
    if (instance) {
      // Remove from source cache
      for (const [source, inst] of sourceCache.entries()) {
        if (inst.windowId === instance.windowId) {
          sourceCache.delete(source)
        }
      }
      log.info(`Unregistered iframe: ${instance.extension.name} (windowId: ${instance.windowId})`)
    }
    iframeRegistry.delete(iframe)
    log.info(`After unregister - registry size: ${iframeRegistry.size}`)
  }

  /**
   * Find extension instance from message event.
   * First tries origin decoding (Desktop), then contentWindow matching (Android fallback).
   * Caches result for faster subsequent lookups.
   */
  const findInstanceFromEvent = (event: MessageEvent): ExtensionInstance | undefined => {
    // Check cache first (performance optimization)
    const cached = sourceCache.get(event.source as Window)
    if (cached) return cached

    // Try to decode from origin (Desktop: haex-extension://<base64>)
    if (event.origin?.startsWith('haex-extension://')) {
      const base64Host = event.origin.replace('haex-extension://', '')
      try {
        const decoded = JSON.parse(atob(base64Host)) as {
          name: string
          publicKey: string
          version: string
        }
        // Find matching extension in registry
        for (const [_, inst] of iframeRegistry.entries()) {
          if (
            inst.extension.name === decoded.name
            && inst.extension.publicKey === decoded.publicKey
            && inst.extension.version === decoded.version
          ) {
            // Cache for future lookups
            sourceCache.set(event.source as Window, inst)
            return inst
          }
        }
      } catch {
        console.warn('[BroadcastStore] Failed to decode origin:', event.origin)
      }
    }

    // Fallback: Match by contentWindow (needed for Android)
    for (const [iframe, inst] of iframeRegistry.entries()) {
      if (iframe.contentWindow === event.source) {
        // Cache for future lookups
        sourceCache.set(event.source as Window, inst)
        return inst
      }
    }

    return undefined
  }

  /**
   * Get extension instance from iframe element
   */
  const getInstanceFromIframe = (iframe: HTMLIFrameElement): ExtensionInstance | undefined => {
    return iframeRegistry.get(iframe)
  }

  /**
   * Get all windows for a specific extension (all instances)
   */
  const getAllWindowsForExtension = (extensionId: string): { instance: ExtensionInstance; window: Window }[] => {
    const result: { instance: ExtensionInstance; window: Window }[] = []
    for (const [iframe, instance] of iframeRegistry.entries()) {
      if (instance.extension.id === extensionId && iframe.contentWindow) {
        result.push({ instance, window: iframe.contentWindow })
      }
    }
    return result
  }

  // ============================================================================
  // Broadcasting Functions
  // ============================================================================

  /**
   * Broadcast context changes to ALL extensions (all instances).
   * Context is public - no permission filtering needed.
   *
   * NOTE: This only broadcasts. Context storage is handled by the caller (uiStore).
   */
  const broadcastContext = async (context: ApplicationContext) => {
    console.log('[BroadcastStore] broadcastContext called with:', context)
    console.log('[BroadcastStore] iframeRegistry size:', iframeRegistry.size)
    console.log('[BroadcastStore] isDesktop:', isDesktop.value)

    const message = {
      type: HAEXTENSION_EVENTS.CONTEXT_CHANGED,
      data: { context },
      timestamp: Date.now(),
    }

    // Send to ALL iframe extension instances
    let iframeSentCount = 0
    for (const [iframe, instance] of iframeRegistry.entries()) {
      console.log('[BroadcastStore] Checking iframe:', {
        extensionName: instance.extension.name,
        windowId: instance.windowId,
        hasContentWindow: !!iframe.contentWindow,
        iframeConnected: iframe.isConnected,
      })
      if (iframe.contentWindow) {
        console.log('[BroadcastStore] Sending context to iframe:', instance.extension.name, instance.windowId)
        iframe.contentWindow.postMessage(message, '*')
        iframeSentCount++
      } else {
        console.warn('[BroadcastStore] Iframe has no contentWindow:', instance.extension.name, instance.windowId)
      }
    }
    console.log('[BroadcastStore] Sent context to', iframeSentCount, 'iframes')

    // On desktop, also broadcast to webview extensions
    if (isDesktop.value) {
      try {
        await invoke(TAURI_COMMANDS.extension.webviewBroadcast, {
          event: HAEXTENSION_EVENTS.CONTEXT_CHANGED,
          payload: { context },
        })
        console.log('[BroadcastStore] Broadcasted context to webview extensions')
      } catch (error) {
        console.error('[BroadcastStore] Failed to broadcast to webview extensions:', error)
      }
    }
  }

  /**
   * Broadcast filtered sync:tables-updated events to ALL instances of each extension.
   * Each extension only receives table names they have database permissions for.
   * Extensions handle deduplication if multiple instances receive the same event.
   *
   * How filtering works:
   * - Frontend passes updated table names to Rust (extension_filter_sync_tables)
   * - Rust loads all installed extensions and their permissions from DB
   * - For each extension, filters tables to only those they have access to:
   *   - Extension's own tables (prefix match: publicKey__name__tableName)
   *   - Tables with explicit DB permissions
   * - Returns map: { extensions: { [extensionId]: [allowedTableNames] } }
   */
  const broadcastSyncTablesUpdated = async (tables: string[]) => {
    if (tables.length === 0) return

    console.log('[BroadcastStore] Broadcasting sync:tables-updated for tables:', tables)

    // Get filtered tables by extension permissions from Rust
    // Rust queries all extensions and their permissions, then filters
    const result = await invoke<FilteredSyncTablesResult>(
      TAURI_COMMANDS.extension.filterSyncTables,
      { tables },
    )

    console.log('[BroadcastStore] Filtered extensions:', Object.keys(result.extensions).length)

    // Send to ALL iframe extension instances
    for (const [iframe, instance] of iframeRegistry.entries()) {
      const extensionId = instance.extension.id

      // Get filtered tables for this extension
      const allowedTables = result.extensions[extensionId]
      if (!allowedTables || allowedTables.length === 0) {
        continue
      }

      if (iframe.contentWindow) {
        const message = {
          type: HAEXTENSION_EVENTS.SYNC_TABLES_UPDATED,
          data: { tables: allowedTables },
          timestamp: Date.now(),
        }
        console.log('[BroadcastStore] Sending sync:tables-updated to:', instance.extension.name, instance.windowId, 'tables:', allowedTables.length)
        iframe.contentWindow.postMessage(message, '*')
      }
    }

    // On desktop, emit to webview extensions
    if (isDesktop.value) {
      try {
        await invoke(TAURI_COMMANDS.extension.emitSyncTables, {
          filteredExtensions: result,
        })
        console.log('[BroadcastStore] Emitted sync:tables-updated to webview extensions')
      } catch (error) {
        console.error('[BroadcastStore] Failed to emit to webview extensions:', error)
      }
    }
  }

  /**
   * Broadcast file change event to ALL instances of extensions with filesystem permissions.
   * Currently broadcasts to all - Rust filtering to be added.
   */
  const broadcastFileChanged = (payload: FileChangePayload) => {
    console.log('[BroadcastStore] Broadcasting file change:', payload.ruleId, payload.changeType)

    const message = {
      type: HAEXTENSION_EVENTS.FILE_CHANGED,
      ruleId: payload.ruleId,
      changeType: payload.changeType,
      path: payload.path,
      timestamp: Date.now(),
    }

    // Send to ALL iframe extension instances (TODO: add permission filtering)
    for (const [iframe, instance] of iframeRegistry.entries()) {
      if (iframe.contentWindow) {
        console.log('[BroadcastStore] Sending file change to:', instance.extension.name, instance.windowId)
        iframe.contentWindow.postMessage(message, '*')
      }
    }
  }

  /**
   * Forward external request to specific extension.
   * Only sends to the FIRST matching iframe to avoid duplicate processing.
   * (External requests are different - they expect a single response)
   */
  const forwardExternalRequest = (payload: ExternalRequestPayload) => {
    const { extensionPublicKey, extensionName } = payload

    log.info(`Forwarding external request to: ${extensionName}, action: ${payload.action}`)
    log.debug('Looking for extension with publicKey:', extensionPublicKey)
    log.debug('Iframe registry size:', iframeRegistry.size)

    // Log all registered iframes for debugging
    let iframeIndex = 0
    for (const [iframe, instance] of iframeRegistry.entries()) {
      log.debug(`Registered iframe ${iframeIndex}:`, {
        extensionName: instance.extension.name,
        extensionPublicKey: instance.extension.publicKey,
        windowId: instance.windowId,
        hasContentWindow: !!iframe.contentWindow,
        iframeConnected: iframe.isConnected,
      })
      iframeIndex++
    }

    // Find first iframe for this extension (external requests need single handler)
    for (const [iframe, instance] of iframeRegistry.entries()) {
      if (
        instance.extension.publicKey === extensionPublicKey
        && instance.extension.name === extensionName
      ) {
        log.info(`Found matching extension iframe: ${instance.extension.name}`)
        if (iframe.contentWindow) {
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
          log.info(`Sent external request to: ${instance.extension.name} (windowId: ${instance.windowId})`)
          iframe.contentWindow.postMessage(message, '*')
          return // Only send to first matching iframe (request expects single response)
        }
        else {
          log.warn('Iframe has no contentWindow!')
        }
      }
    }

    log.warn(`No iframe found for extension: ${extensionName} (publicKey: ${extensionPublicKey})`)
  }

  /**
   * Broadcast LocalSend event to ALL instances of extensions with localsend permission.
   * Uses permission-based filtering like other events.
   */
  const broadcastLocalSendEvent = (eventType: string, payload: unknown) => {
    console.log('[BroadcastStore] Broadcasting LocalSend event:', eventType)

    const message = {
      type: eventType,
      data: payload,
      timestamp: Date.now(),
    }

    // Send to ALL iframe extension instances (TODO: Filter by localsend permission)
    for (const [iframe, instance] of iframeRegistry.entries()) {
      if (iframe.contentWindow) {
        iframe.contentWindow.postMessage(message, '*')
      }
    }
  }

  // ============================================================================
  // Tauri Event Listeners Setup
  // ============================================================================

  let eventListenersRegistered = false
  const unlistenFns: UnlistenFn[] = []

  /**
   * Setup Tauri event listeners for forwarding to iframes
   */
  const setupEventListeners = async () => {
    if (eventListenersRegistered) {
      log.debug('Event listeners already registered, skipping')
      return
    }
    eventListenersRegistered = true

    log.info('========== SETTING UP EVENT LISTENERS ==========')
    log.debug('EXTERNAL_EVENTS.REQUEST:', EXTERNAL_EVENTS.REQUEST)
    log.debug('isDesktop:', isDesktop.value)

    try {
      // Listen for external requests
      unlistenFns.push(
        await listen<ExternalRequestPayload>(EXTERNAL_EVENTS.REQUEST, (event) => {
          log.info('========== EXTERNAL REQUEST RECEIVED ==========')
          log.info(`Extension: ${event.payload.extensionName}`)
          log.info(`Action: ${event.payload.action}`)
          log.debug('Request ID:', event.payload.requestId)
          log.debug('Full payload:', JSON.stringify(event.payload))
          log.debug('Current iframe registry size:', iframeRegistry.size)
          forwardExternalRequest(event.payload)
        }),
      )

      // Listen for file change events from native file watcher
      unlistenFns.push(
        await listen<FileChangePayload>(HAEXTENSION_EVENTS.FILE_CHANGED, (event) => {
          console.log('[BroadcastStore] Received file change from Tauri:', event.payload)
          broadcastFileChanged(event.payload)
        }),
      )

      // Listen for LocalSend events
      unlistenFns.push(
        await listen(LOCALSEND_EVENTS.transferRequest, (event) => {
          broadcastLocalSendEvent(LOCALSEND_EVENTS.transferRequest, event.payload)
        }),
      )

      unlistenFns.push(
        await listen(LOCALSEND_EVENTS.transferProgress, (event) => {
          broadcastLocalSendEvent(LOCALSEND_EVENTS.transferProgress, event.payload)
        }),
      )

      unlistenFns.push(
        await listen(LOCALSEND_EVENTS.transferComplete, (event) => {
          broadcastLocalSendEvent(LOCALSEND_EVENTS.transferComplete, event.payload)
        }),
      )

      unlistenFns.push(
        await listen(LOCALSEND_EVENTS.transferFailed, (event) => {
          broadcastLocalSendEvent(LOCALSEND_EVENTS.transferFailed, event.payload)
        }),
      )

      unlistenFns.push(
        await listen(LOCALSEND_EVENTS.deviceDiscovered, (event) => {
          broadcastLocalSendEvent(LOCALSEND_EVENTS.deviceDiscovered, event.payload)
        }),
      )

      unlistenFns.push(
        await listen(LOCALSEND_EVENTS.deviceLost, (event) => {
          broadcastLocalSendEvent(LOCALSEND_EVENTS.deviceLost, event.payload)
        }),
      )

      console.log('[BroadcastStore] Event listeners registered')
    } catch (error) {
      console.error('[BroadcastStore] Failed to setup event listeners:', error)
    }
  }

  /**
   * Cleanup event listeners
   */
  const cleanup = () => {
    for (const unlisten of unlistenFns) {
      unlisten()
    }
    unlistenFns.length = 0
    eventListenersRegistered = false

    // Clear all registries
    iframeRegistry.clear()
    sourceCache.clear()

    console.log('[BroadcastStore] Cleaned up')
  }

  return {
    // Iframe registry management
    registerIframe,
    unregisterIframe,
    findInstanceFromEvent,
    getInstanceFromIframe,
    getAllWindowsForExtension,

    // Expose registry for backwards compatibility with extensionMessageHandler
    iframeRegistry,
    sourceCache,

    // Broadcasting functions
    broadcastContext,
    broadcastSyncTablesUpdated,
    broadcastFileChanged,
    forwardExternalRequest,
    broadcastLocalSendEvent,

    // Setup and cleanup
    setupEventListeners,
    cleanup,
  }
})
