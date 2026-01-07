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

    // Find extension instance using broadcast store
    let instance: ExtensionInstance | undefined

    // First try: Find by contentWindow match (works for sandboxed iframes with origin "null")
    for (const [iframe, inst] of broadcastStore.iframeRegistry.entries()) {
      if (iframe.contentWindow === event.source) {
        instance = inst
        // Cache for future lookups
        broadcastStore.sourceCache.set(event.source as Window, inst)
        break
      }
    }

    // Try to decode extension info from origin (desktop custom protocol)
    if (!instance && event.origin) {
      let base64Host: string | null = null

      if (event.origin.startsWith(EXTENSION_PROTOCOL_PREFIX)) {
        // Desktop format: haex-extension://<base64>
        base64Host = event.origin.replace(EXTENSION_PROTOCOL_PREFIX, '')
      } else if (
        event.origin === `http://${EXTENSION_PROTOCOL_NAME}.localhost`
      ) {
        // Android format: http://haex-extension.localhost/{base64} (origin doesn't contain extension info)
        // Fallback to single iframe mode if contentWindow match didn't work
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

    // Fallback: Try to find extension instance from cache
    if (!instance) {
      instance = broadcastStore.sourceCache.get(event.source as Window)
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

  // Initialize the context store - this starts watching for context changes
  // and will broadcast to extensions when theme/locale/deviceId changes
  useExtensionContextStore()

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

  // Track if we've already registered this iframe
  let registeredIframe: HTMLIFrameElement | null = null

  // Registriere dieses IFrame via broadcast store - only once when iframe becomes available
  watch(
    [iframeRef, extension],
    ([iframe, ext]) => {
      if (iframe && ext && iframe !== registeredIframe) {
        registeredIframe = iframe
        broadcastStore.registerIframe(iframe, ext, windowId.value)
      }
    },
    { immediate: true },
  )

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
