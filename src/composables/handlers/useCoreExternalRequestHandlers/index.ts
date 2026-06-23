import { listen } from '@tauri-apps/api/event'
import { createOnceListener } from '@/lib/once-listener'
import {
  handleCreateItemAsync,
  handleGetItemsAsync,
  handleGetPasswordConfigAsync,
  handleGetPasswordPresetsAsync,
  handleGetTotpAsync,
  handleUpdateItemAsync,
} from './passwords'
import {
  handlePasskeyCreateAsync,
  handlePasskeyGetAsync,
  handlePasskeyListAsync,
} from './passkeys'
import { errorResponse, respondAsync, toErrorMessage } from './shared'
import { CORE_METHODS, CORE_REQUEST_EVENT } from './types'
import type { ExternalCoreRequest, ExternalCoreResponse } from './types'

const dispatchAsync = async (request: ExternalCoreRequest): Promise<ExternalCoreResponse> => {
  try {
    switch (request.action) {
      case CORE_METHODS.GET_ITEMS:
        return await handleGetItemsAsync(request)
      case CORE_METHODS.GET_TOTP:
        return await handleGetTotpAsync(request)
      case CORE_METHODS.CREATE_ITEM:
        return await handleCreateItemAsync(request)
      case CORE_METHODS.UPDATE_ITEM:
        return await handleUpdateItemAsync(request)
      case CORE_METHODS.GET_PASSWORD_CONFIG:
        return await handleGetPasswordConfigAsync(request)
      case CORE_METHODS.GET_PASSWORD_PRESETS:
        return await handleGetPasswordPresetsAsync(request)
      case CORE_METHODS.PASSKEY_CREATE:
        return await handlePasskeyCreateAsync(request)
      case CORE_METHODS.PASSKEY_GET:
        return await handlePasskeyGetAsync(request)
      case CORE_METHODS.PASSKEY_LIST:
        return await handlePasskeyListAsync(request)
      default:
        return errorResponse(request.requestId, `Unknown core action: ${request.action}`)
    }
  } catch (error) {
    console.error(`[core] handler failed for ${request.action}:`, error)
    return errorResponse(request.requestId, toErrorMessage(error))
  }
}

export const useCoreExternalRequestHandlers = () => {
  const listener = createOnceListener(() =>
    listen<ExternalCoreRequest>(CORE_REQUEST_EVENT, async (event) => {
      const response = await dispatchAsync(event.payload)
      await respondAsync(response).catch((err) => {
        console.error('[core] failed to send response:', err)
      })
    }),
  )

  return { initAsync: listener.initAsync, dispose: listener.dispose }
}
