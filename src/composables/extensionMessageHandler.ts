// composables/extensionMessageHandler.ts
/**
 * Extension Message Handler
 *
 * Since SDK 3.0 all extension ↔ main-window messaging flows through a
 * dedicated `MessagePort` established during iframe registration (see
 * `broadcast.ts`). This module no longer installs a `window.addEventListener`
 * — the port is the trust boundary, and every inbound request already carries
 * an unambiguous extension identity via its owning port.
 *
 * What lives here:
 *   - `handleExtensionRequestAsync` — routes a single request to the right
 *     handler (database / filesystem / web / permissions / …).
 *   - `useExtensionMessageHandler` — Vue composable that registers context
 *     getters and wires an iframe into the broadcast store on mount.
 */
import type { IHaexSpaceExtension } from '~/types/haexspace'
import { TAURI_COMMANDS } from '@haex-space/vault-sdk'
import { handleDatabaseMethodAsync } from './handlers/database'
import { handleFilesystemMethodAsync } from './handlers/filesystem'
import { handleWebMethodAsync } from './handlers/web'
import { handlePermissionsMethodAsync } from './handlers/permissions'
import { handleContextMethodAsync, setContextGetters } from './handlers/context'
import { handleWebStorageMethodAsync } from './handlers/webStorage'
import { handleRemoteStorageMethodAsync } from './handlers/remoteStorage'
import { handleSpacesMethodAsync } from './handlers/spaces'
import { handleLoggingMethodAsync } from './handlers/logging'
import { handleShellMethodAsync } from './handlers/shell'
import type { ExtensionRequest, ExtensionInstance } from './handlers/types'
import { useExtensionBroadcastStore } from '~/stores/extensions/broadcast'

/**
 * Shape of a response sent back to the extension over its MessagePort.
 * Either `result` or `error` is populated, never both. The `id` matches the
 * `id` the extension included with the request.
 */
export interface ExtensionResponse {
  id: string
  result?: unknown
  error?: {
    code: string
    message: string
    details?: unknown
  }
}

/**
 * Route a single request from a known extension to the appropriate handler
 * and return the response to send back on the port.
 *
 * Never throws — any handler error is wrapped into an `ExtensionResponse.error`
 * so callers can post the response unconditionally. The `INVALID_REQUEST`
 * error code surfaces malformed requests (missing id or method) so the
 * extension sees a deterministic failure rather than a silent drop.
 */
export const handleExtensionRequestAsync = async (
  request: ExtensionRequest,
  instance: ExtensionInstance,
): Promise<ExtensionResponse> => {
  if (!request?.id || !request?.method) {
    return {
      id: request?.id ?? '',
      error: {
        code: 'INVALID_REQUEST',
        message: 'Request must include id and method',
        details: request,
      },
    }
  }

  try {
    const method = request.method
    let result: unknown

    if (method === TAURI_COMMANDS.extension.getContext) {
      result = await handleContextMethodAsync(request)
    }
    else if (
      method === TAURI_COMMANDS.webStorage.getItem
      || method === TAURI_COMMANDS.webStorage.setItem
      || method === TAURI_COMMANDS.webStorage.removeItem
      || method === TAURI_COMMANDS.webStorage.clear
      || method === TAURI_COMMANDS.webStorage.keys
    ) {
      result = await handleWebStorageMethodAsync(request, instance)
    }
    else if (
      method === TAURI_COMMANDS.database.query
      || method === TAURI_COMMANDS.database.execute
      || method === TAURI_COMMANDS.database.transaction
      || method === TAURI_COMMANDS.database.registerMigrations
    ) {
      result = await handleDatabaseMethodAsync(request, instance.extension)
    }
    else if (
      method === TAURI_COMMANDS.filesystem.saveFile
      || method === TAURI_COMMANDS.filesystem.openFile
      || method === TAURI_COMMANDS.filesystem.showImage
      || method.startsWith('extension_filesystem_')
    ) {
      result = await handleFilesystemMethodAsync(request, instance.extension)
    }
    else if (
      method === TAURI_COMMANDS.web.fetch
      || method === TAURI_COMMANDS.web.open
    ) {
      result = await handleWebMethodAsync(request, instance.extension)
    }
    else if (method.startsWith('extension_permissions_')) {
      result = await handlePermissionsMethodAsync(request, instance.extension)
    }
    else if (method.startsWith('extension_remote_storage_')) {
      result = await handleRemoteStorageMethodAsync(request, instance.extension)
    }
    else if (method.startsWith('extension_space_')) {
      result = await handleSpacesMethodAsync(request, instance.extension)
    }
    else if (method.startsWith('extension_logging_')) {
      result = await handleLoggingMethodAsync(request, instance.extension)
    }
    else if (method.startsWith('extension_shell_')) {
      result = await handleShellMethodAsync(request, instance.extension)
    }
    else {
      throw new Error(`Unknown method: ${method}`)
    }

    return { id: request.id, result }
  }
  catch (error) {
    console.error('[ExtensionHandler] Extension request error:', error)
    return {
      id: request.id,
      error: {
        code: 'INTERNAL_ERROR',
        message: error instanceof Error ? error.message : 'Unknown error',
        details: error,
      },
    }
  }
}

export const useExtensionMessageHandler = (
  iframeRef: Ref<HTMLIFrameElement | undefined | null>,
  extension: ComputedRef<IHaexSpaceExtension | undefined | null>,
  windowId: Ref<string>,
) => {
  const broadcastStore = useExtensionBroadcastStore()

  // Initialize the context store — starts watching for context changes and
  // broadcasts them to extensions when theme/locale/deviceId updates.
  useExtensionContextStore()

  // Initialize context getters for non-setup callers (e.g. handlers).
  const { currentTheme } = storeToRefs(useUiStore())
  const { locale } = useI18n()
  const { platform, deviceId } = useDeviceStore()
  setContextGetters({
    getTheme: () => currentTheme.value?.value || 'system',
    getLocale: () => locale.value,
    getPlatform: () => platform,
    getDeviceId: () => deviceId,
  })

  // Make sure the broadcast store's Tauri event listeners are up — these
  // translate Rust-emitted events (file-change, shell output, etc.) into
  // per-extension port dispatches.
  broadcastStore.setupEventListeners()

  let registeredIframe: HTMLIFrameElement | null = null

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

  onUnmounted(() => {
    if (iframeRef.value) {
      broadcastStore.unregisterIframe(iframeRef.value)
    }
  })
}

export const registerExtensionIFrame = (
  iframe: HTMLIFrameElement,
  extension: IHaexSpaceExtension,
  windowId: string,
) => {
  const broadcastStore = useExtensionBroadcastStore()
  broadcastStore.setupEventListeners()
  broadcastStore.registerIframe(iframe, extension, windowId)
}

export const unregisterExtensionIFrame = (iframe: HTMLIFrameElement) => {
  const broadcastStore = useExtensionBroadcastStore()
  broadcastStore.unregisterIframe(iframe)
}
