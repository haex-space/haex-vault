// composables/extensionMessageHandler.ts
/**
 * Extension Message Handler
 *
 * Handles incoming postMessage requests from extension iframes.
 * Routes requests to appropriate handlers (database, filesystem, web, etc.)
 *
 * Broadcasting is handled by the extensionBroadcastStore.
 * This composable only handles:
 * - Iframe registration (delegates to broadcast store)
 * - Message reception and routing
 * - Response sending
 */
import type { IHaexSpaceExtension } from '~/types/haexspace'
import {
  TAURI_COMMANDS,
  HAEXSPACE_MESSAGE_TYPES,
} from '@haex-space/vault-sdk'
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
  handleLocalSendMethodAsync,
  setContextGetters,
  type ExtensionRequest,
  type ExtensionInstance,
} from './handlers'
import { useExtensionBroadcastStore } from '~/stores/extensions/broadcast'

// Globaler Handler - nur einmal registriert
let globalHandlerRegistered = false

const registerGlobalMessageHandler = () => {
  if (globalHandlerRegistered) return

  console.log('[ExtensionHandler] Registering global message handler')

  // Get broadcast store for registry access and event listener setup
  const broadcastStore = useExtensionBroadcastStore()

  // Setup Tauri event listeners for forwarding to iframes
  broadcastStore.setupEventListeners()

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

    // Find extension instance using broadcast store
    let instance: ExtensionInstance | undefined

    // Debug: Find which extension sent this message
    let sourceInfo = 'unknown source'
    for (const [iframe, inst] of broadcastStore.iframeRegistry.entries()) {
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
        if (broadcastStore.iframeRegistry.size === 1) {
          const entry = Array.from(broadcastStore.iframeRegistry.entries())[0]
          if (entry) {
            const [_, inst] = entry
            instance = inst
            broadcastStore.sourceCache.set(event.source as Window, inst)
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
          for (const [_, inst] of broadcastStore.iframeRegistry.entries()) {
            if (
              inst.extension.name === decodedInfo.name &&
              inst.extension.publicKey === decodedInfo.publicKey &&
              inst.extension.version === decodedInfo.version
            ) {
              instance = inst
              // Cache for future lookups
              broadcastStore.sourceCache.set(event.source as Window, inst)
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
      instance = broadcastStore.sourceCache.get(event.source as Window)

      // If not cached yet, find by matching iframe.contentWindow to event.source
      if (!instance) {
        for (const [iframe, inst] of broadcastStore.iframeRegistry.entries()) {
          if (iframe.contentWindow === event.source) {
            instance = inst
            // Cache for future lookups
            broadcastStore.sourceCache.set(event.source as Window, inst)
            console.log(
              '[ExtensionHandler] Registered instance via contentWindow match:',
              inst.windowId,
            )
            break
          }
        }
      }
    }

    if (!instance) {
      console.warn(
        '[ExtensionHandler] Could not identify extension instance from event.source.',
        'Registered iframes:',
        broadcastStore.iframeRegistry.size,
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
      } else if (request.method.startsWith('localsend_')) {
        result = await handleLocalSendMethodAsync(request, instance.extension)
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
  // Get broadcast store for registration
  const broadcastStore = useExtensionBroadcastStore()

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

  // Registriere dieses IFrame via broadcast store
  watchEffect(() => {
    if (iframeRef.value && extension.value) {
      broadcastStore.registerIframe(iframeRef.value, extension.value, windowId.value)
    }
  })

  // Cleanup beim Unmount
  onUnmounted(() => {
    if (iframeRef.value) {
      broadcastStore.unregisterIframe(iframeRef.value)
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

  // Register via broadcast store
  const broadcastStore = useExtensionBroadcastStore()
  broadcastStore.registerIframe(iframe, extension, windowId)
}

export const unregisterExtensionIFrame = (iframe: HTMLIFrameElement) => {
  const broadcastStore = useExtensionBroadcastStore()
  broadcastStore.unregisterIframe(iframe)
}

// Re-export types for backwards compatibility
export type { ExtensionInstance }
