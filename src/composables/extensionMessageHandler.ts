// composables/extensionMessageHandler.ts
import type { IHaexSpaceExtension } from '~/types/haexspace'
import {
  TAURI_COMMANDS,
  HAEXTENSION_EVENTS,
  HAEXSPACE_MESSAGE_TYPES,
  EXTERNAL_EVENTS,
} from '@haex-space/vault-sdk'
import { listen } from '@tauri-apps/api/event'
import {
  EXTENSION_PROTOCOL_NAME,
  EXTENSION_PROTOCOL_PREFIX,
} from '~/config/constants'
import {
  handleDatabaseMethodAsync,
  handleFilesystemMethodAsync,
  handleWebMethodAsync,
  handlePermissionsMethodAsync,
  handleContextMethodAsync,
  handleWebStorageMethodAsync,
  handleRemoteStorageMethodAsync,
  setContextGetters,
  type ExtensionRequest,
  type ExtensionInstance,
} from './handlers'

// Globaler Handler - nur einmal registriert
let globalHandlerRegistered = false
const iframeRegistry = new Map<HTMLIFrameElement, ExtensionInstance>()
// Map event.source (WindowProxy) to extension instance for sandbox-compatible matching
const sourceRegistry = new Map<Window, ExtensionInstance>()
// Reverse map: window ID to Window for broadcasting (supports multiple windows per extension)
const windowIdToWindowMap = new Map<string, Window>()

const registerGlobalMessageHandler = () => {
  if (globalHandlerRegistered) return

  console.log('[ExtensionHandler] Registering global message handler')

  // Setup external request listener for iframe forwarding
  setupExternalRequestListener()

  window.addEventListener('message', async (event: MessageEvent) => {
    // Log ALL messages first for debugging
    console.log('[ExtensionHandler] Raw message received:', {
      origin: event.origin,
      dataType: typeof event.data,
      data: event.data,
      hasSource: !!event.source,
    })

    // Ignore console.forward messages - they're handled elsewhere
    if (event.data?.type === HAEXSPACE_MESSAGE_TYPES.CONSOLE_FORWARD) {
      return
    }

    // Handle debug messages for Android debugging
    if (event.data?.type === HAEXSPACE_MESSAGE_TYPES.DEBUG) {
      console.log('[ExtensionHandler] DEBUG MESSAGE FROM EXTENSION:', event.data.data)
      return
    }

    const request = event.data as ExtensionRequest

    console.log('[ExtensionHandler] Processing extension message:', {
      origin: event.origin,
      method: request?.method,
      id: request?.id,
      hasSource: !!event.source,
    })

    // Find extension instance by decoding event.origin (works with sandboxed iframes)
    // Origin formats:
    // - Desktop: haex-extension://<base64>
    // - Android: http://haex-extension.localhost (need to check request URL for base64)
    let instance: ExtensionInstance | undefined

    // Debug: Find which extension sent this message
    let sourceInfo = 'unknown source'
    for (const [iframe, inst] of iframeRegistry.entries()) {
      if (iframe.contentWindow === event.source) {
        sourceInfo = `${inst.extension.name} (${inst.windowId})`
        break
      }
    }
    console.log(
      '[ExtensionHandler] Received message from:',
      sourceInfo,
      'Method:',
      request.method,
    )

    // Try to decode extension info from origin
    if (event.origin) {
      let base64Host: string | null = null

      if (event.origin.startsWith(EXTENSION_PROTOCOL_PREFIX)) {
        // Desktop format: haex-extension://<base64>
        base64Host = event.origin.replace(EXTENSION_PROTOCOL_PREFIX, '')
        console.log(
          '[ExtensionHandler] Extracted base64 (custom protocol):',
          base64Host,
        )
      } else if (
        event.origin === `http://${EXTENSION_PROTOCOL_NAME}.localhost`
      ) {
        // Android format: http://haex-extension.localhost/{base64} (origin doesn't contain extension info)
        // We need to identify extension by iframe source or fallback to single-extension mode
        console.log(
          `[ExtensionHandler] Android format detected (http://${EXTENSION_PROTOCOL_NAME}.localhost)`,
        )
        // Fallback to single iframe mode
        if (iframeRegistry.size === 1) {
          const entry = Array.from(iframeRegistry.entries())[0]
          if (entry) {
            const [_, inst] = entry
            instance = inst
            sourceRegistry.set(event.source as Window, inst)
            windowIdToWindowMap.set(inst.windowId, event.source as Window)
          }
        }
      }

      if (base64Host && base64Host !== 'localhost') {
        try {
          const decodedInfo = JSON.parse(atob(base64Host)) as {
            name: string
            publicKey: string
            version: string
          }

          // Find matching extension in registry
          for (const [_, inst] of iframeRegistry.entries()) {
            if (
              inst.extension.name === decodedInfo.name &&
              inst.extension.publicKey === decodedInfo.publicKey &&
              inst.extension.version === decodedInfo.version
            ) {
              instance = inst
              // Register for future lookups
              sourceRegistry.set(event.source as Window, inst)
              windowIdToWindowMap.set(inst.windowId, event.source as Window)
              break
            }
          }
        } catch (e) {
          console.error('[ExtensionHandler] Failed to decode origin:', e)
        }
      }
    }

    // Fallback: Try to find extension instance by event.source (for localhost origin or legacy)
    if (!instance) {
      instance = sourceRegistry.get(event.source as Window)

      // If not registered yet, find by matching iframe.contentWindow to event.source
      if (!instance) {
        for (const [iframe, inst] of iframeRegistry.entries()) {
          if (iframe.contentWindow === event.source) {
            instance = inst
            // Register for future lookups
            sourceRegistry.set(event.source as Window, inst)
            windowIdToWindowMap.set(inst.windowId, event.source as Window)
            console.log(
              '[ExtensionHandler] Registered instance via contentWindow match:',
              inst.windowId,
            )
            break
          }
        }
      } else if (instance && !windowIdToWindowMap.has(instance.windowId)) {
        // Also register in reverse map for broadcasting
        windowIdToWindowMap.set(instance.windowId, event.source as Window)
      }
    }

    if (!instance) {
      console.warn(
        '[ExtensionHandler] Could not identify extension instance from event.source.',
        'Registered iframes:',
        iframeRegistry.size,
      )
      return // Message ist nicht von einem registrierten IFrame
    }

    if (!request.id || !request.method) {
      console.error('[ExtensionHandler] Invalid extension request:', request)
      return
    }

    try {
      let result: unknown

      // Check specific methods first, then use direct routing to handlers
      if (request.method === TAURI_COMMANDS.extension.getContext) {
        result = await handleContextMethodAsync(request)
      } else if (
        request.method === TAURI_COMMANDS.webStorage.getItem ||
        request.method === TAURI_COMMANDS.webStorage.setItem ||
        request.method === TAURI_COMMANDS.webStorage.removeItem ||
        request.method === TAURI_COMMANDS.webStorage.clear ||
        request.method === TAURI_COMMANDS.webStorage.keys
      ) {
        result = await handleWebStorageMethodAsync(request, instance)
      } else if (
        request.method === TAURI_COMMANDS.database.query ||
        request.method === TAURI_COMMANDS.database.execute ||
        request.method === TAURI_COMMANDS.database.transaction ||
        request.method === TAURI_COMMANDS.database.registerMigrations
      ) {
        result = await handleDatabaseMethodAsync(request, instance.extension)
      } else if (
        request.method === TAURI_COMMANDS.filesystem.saveFile ||
        request.method === TAURI_COMMANDS.filesystem.openFile ||
        request.method === TAURI_COMMANDS.filesystem.showImage ||
        request.method.startsWith('extension_filesystem_')
      ) {
        result = await handleFilesystemMethodAsync(request, instance.extension)
      } else if (
        request.method === TAURI_COMMANDS.web.fetch ||
        request.method === TAURI_COMMANDS.web.open
      ) {
        result = await handleWebMethodAsync(request, instance.extension)
      } else if (request.method.startsWith('extension_permissions_')) {
        result = await handlePermissionsMethodAsync(request, instance.extension)
      } else if (request.method.startsWith('extension_remote_storage_')) {
        result = await handleRemoteStorageMethodAsync(request, instance.extension)
      } else {
        throw new Error(`Unknown method: ${request.method}`)
      }

      // Use event.source instead of contentWindow to work with sandboxed iframes
      // For sandboxed iframes, event.origin is "null" (string), which is not valid for postMessage
      const targetOrigin = event.origin === 'null' ? '*' : event.origin || '*'

      ;(event.source as Window)?.postMessage(
        {
          id: request.id,
          result,
        },
        targetOrigin,
      )
    } catch (error) {
      console.error('[ExtensionHandler] Extension request error:', error)

      // Use event.source instead of contentWindow to work with sandboxed iframes
      // For sandboxed iframes, event.origin is "null" (string), which is not valid for postMessage
      const targetOrigin = event.origin === 'null' ? '*' : event.origin || '*'

      ;(event.source as Window)?.postMessage(
        {
          id: request.id,
          error: {
            code: 'INTERNAL_ERROR',
            message: error instanceof Error ? error.message : 'Unknown error',
            details: error,
          },
        },
        targetOrigin,
      )
    }
  })

  globalHandlerRegistered = true
}

export const useExtensionMessageHandler = (
  iframeRef: Ref<HTMLIFrameElement | undefined | null>,
  extension: ComputedRef<IHaexSpaceExtension | undefined | null>,
  windowId: Ref<string>,
) => {
  // Initialize context getters (can use composables here because we're in setup)
  const { currentTheme } = storeToRefs(useUiStore())
  const { locale } = useI18n()
  const { platform, deviceId } = useDeviceStore()
  // Store getters for use outside setup context
  setContextGetters({
    getTheme: () => currentTheme.value?.value || 'system',
    getLocale: () => locale.value,
    getPlatform: () => platform,
    getDeviceId: () => deviceId,
  })

  // Registriere globalen Handler beim ersten Aufruf
  registerGlobalMessageHandler()

  // Registriere dieses IFrame
  watchEffect(() => {
    if (iframeRef.value && extension.value) {
      iframeRegistry.set(iframeRef.value, {
        extension: extension.value,
        windowId: windowId.value,
      })
    }
  })

  // Cleanup beim Unmount
  onUnmounted(() => {
    if (iframeRef.value) {
      const instance = iframeRegistry.get(iframeRef.value)
      if (instance) {
        // Remove from all maps
        windowIdToWindowMap.delete(instance.windowId)
        for (const [source, inst] of sourceRegistry.entries()) {
          if (inst.windowId === instance.windowId) {
            sourceRegistry.delete(source)
          }
        }
      }
      iframeRegistry.delete(iframeRef.value)
    }
  })
}

// Export Funktion für manuelle IFrame-Registrierung (kein Composable!)
export const registerExtensionIFrame = (
  iframe: HTMLIFrameElement,
  extension: IHaexSpaceExtension,
  windowId: string,
) => {
  // Stelle sicher, dass der globale Handler registriert ist
  registerGlobalMessageHandler()

  // Note: Context getters should be initialized via useExtensionMessageHandler first

  iframeRegistry.set(iframe, { extension, windowId })
}

export const unregisterExtensionIFrame = (iframe: HTMLIFrameElement) => {
  // Also remove from source registry and instance map
  const instance = iframeRegistry.get(iframe)
  if (instance) {
    // Find and remove all sources pointing to this instance
    for (const [source, inst] of sourceRegistry.entries()) {
      if (inst.windowId === instance.windowId) {
        sourceRegistry.delete(source)
      }
    }
    // Remove from instance-to-window map
    windowIdToWindowMap.delete(instance.windowId)
  }
  iframeRegistry.delete(iframe)
}

// Export function to get Window for a specific instance (for broadcasting)
export const getInstanceWindow = (windowId: string): Window | undefined => {
  return windowIdToWindowMap.get(windowId)
}

// Get all windows for an extension (all instances)
export const getAllInstanceWindows = (extensionId: string): Window[] => {
  const windows: Window[] = []
  for (const [_, instance] of iframeRegistry.entries()) {
    if (instance.extension.id === extensionId) {
      const win = windowIdToWindowMap.get(instance.windowId)
      if (win) {
        windows.push(win)
      }
    }
  }
  return windows
}

// Deprecated - kept for backwards compatibility
export const getExtensionWindow = (extensionId: string): Window | undefined => {
  // Return first window for this extension
  return getAllInstanceWindows(extensionId)[0]
}

// Broadcast context changes to all extension instances
export const broadcastContextToAllExtensions = (context: {
  theme: string
  locale: string
  platform?: string
}) => {
  const message = {
    type: HAEXTENSION_EVENTS.CONTEXT_CHANGED,
    data: { context },
    timestamp: Date.now(),
  }

  console.log(
    '[ExtensionHandler] Broadcasting context to all extensions:',
    context,
  )

  // Send to all registered extension windows
  for (const [_, instance] of iframeRegistry.entries()) {
    const win = windowIdToWindowMap.get(instance.windowId)
    if (win) {
      console.log(
        '[ExtensionHandler] Sending context to:',
        instance.extension.name,
        instance.windowId,
      )
      win.postMessage(message, '*')
    }
  }
}

// External request payload from Tauri event
interface ExternalRequestPayload {
  requestId: string
  publicKey: string
  action: string
  payload: unknown
  extensionPublicKey: string
  extensionName: string
}

// File change event payload from Tauri event (from native file watcher)
interface FileChangePayload {
  ruleId: string
  changeType: 'created' | 'modified' | 'removed' | 'any'
  path?: string
}

/**
 * Sends a message to the first window of each unique extension.
 * This ensures each extension receives the message exactly once,
 * even if multiple windows are open for the same extension.
 *
 * @param message - The message object to send via postMessage
 * @param logPrefix - Prefix for log messages (e.g., 'sync:tables-updated')
 */
const broadcastToFirstWindowOfEachExtension = (
  message: Record<string, unknown>,
  logPrefix: string,
) => {
  // Track which extensions we've already sent to (by publicKey)
  const sentToExtensions = new Set<string>()

  for (const [iframe, instance] of iframeRegistry.entries()) {
    // Only send once per extension (first window wins)
    if (sentToExtensions.has(instance.extension.publicKey)) {
      continue
    }

    const win = windowIdToWindowMap.get(instance.windowId) || iframe.contentWindow
    if (win) {
      console.log(
        `[ExtensionHandler] Sending ${logPrefix} to:`,
        instance.extension.name,
        instance.windowId,
      )
      win.postMessage(message, '*')
      sentToExtensions.add(instance.extension.publicKey)
    }
  }
}

// Forward external requests from Tauri to iframe extensions
// Only sends to the FIRST matching iframe to avoid duplicate processing
const forwardExternalRequestToIframe = (payload: ExternalRequestPayload) => {
  const { extensionPublicKey, extensionName } = payload

  console.log(
    '[ExtensionHandler] Forwarding external request to iframe:',
    extensionName,
    'action:',
    payload.action,
  )

  // Find the first iframe for this extension (by publicKey and name)
  for (const [iframe, instance] of iframeRegistry.entries()) {
    if (
      instance.extension.publicKey === extensionPublicKey
      && instance.extension.name === extensionName
    ) {
      const win = windowIdToWindowMap.get(instance.windowId) || iframe.contentWindow
      if (win) {
        // Send as SDK-compatible event format
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
        console.log(
          '[ExtensionHandler] Sending external request to:',
          instance.extension.name,
          instance.windowId,
        )
        win.postMessage(message, '*')
        return // Only send to first matching iframe
      }
    }
  }

  console.warn(
    '[ExtensionHandler] No iframe found for extension:',
    extensionName,
    extensionPublicKey,
  )
}

// Forward file change events from Tauri to all extension iframes
// Extensions can filter by ruleId on their side
const forwardFileChangeToIframes = (payload: FileChangePayload) => {
  console.log(
    '[ExtensionHandler] Forwarding file change event:',
    payload.ruleId,
    payload.changeType,
    payload.path,
  )

  // Send to all registered extension windows
  for (const [iframe, instance] of iframeRegistry.entries()) {
    const win = windowIdToWindowMap.get(instance.windowId) || iframe.contentWindow
    if (win) {
      // Send as SDK-compatible event format
      const message = {
        type: HAEXTENSION_EVENTS.FILE_CHANGED,
        ruleId: payload.ruleId,
        changeType: payload.changeType,
        path: payload.path,
        timestamp: Date.now(),
      }
      console.log(
        '[ExtensionHandler] Sending file change to:',
        instance.extension.name,
        instance.windowId,
      )
      win.postMessage(message, '*')
    }
  }
}

// Payload type for sync:tables-updated event
interface SyncTablesUpdatedPayload {
  tables: string[]
}

// Forward sync tables updated events from Tauri to extension iframes
// Only sends to the FIRST window of each extension to avoid duplicate processing
const forwardSyncTablesUpdatedToIframes = (payload: SyncTablesUpdatedPayload) => {
  console.log(
    '[ExtensionHandler] Forwarding sync:tables-updated event:',
    payload.tables,
  )

  const message = {
    type: HAEXTENSION_EVENTS.SYNC_TABLES_UPDATED,
    data: {
      tables: payload.tables,
    },
    timestamp: Date.now(),
  }

  broadcastToFirstWindowOfEachExtension(message, 'sync:tables-updated')
}

// Setup Tauri event listeners for external requests and file changes (for iframe extensions)
let eventListenersRegistered = false

const setupExternalRequestListener = async () => {
  if (eventListenersRegistered) return

  try {
    // Listen for external requests
    await listen<ExternalRequestPayload>(EXTERNAL_EVENTS.REQUEST, (event) => {
      console.log('[ExtensionHandler] Received external request from Tauri:', event.payload)
      forwardExternalRequestToIframe(event.payload)
    })

    // Listen for file change events from native file watcher
    await listen<FileChangePayload>(HAEXTENSION_EVENTS.FILE_CHANGED, (event) => {
      console.log('[ExtensionHandler] Received file change from Tauri:', event.payload)
      forwardFileChangeToIframes(event.payload)
    })

    // Listen for sync tables updated events (from CRDT pull)
    await listen<SyncTablesUpdatedPayload>('sync:tables-updated', (event) => {
      console.log('[ExtensionHandler] Received sync:tables-updated from Tauri:', event.payload)
      forwardSyncTablesUpdatedToIframes(event.payload)
    })

    eventListenersRegistered = true
    console.log('[ExtensionHandler] Event listeners registered (external requests + file changes + sync updates)')
  }
  catch (error) {
    console.error('[ExtensionHandler] Failed to setup event listeners:', error)
  }
}

