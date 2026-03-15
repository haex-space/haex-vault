import { invoke } from '@tauri-apps/api/core'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { ExtensionRequest } from './types'

export async function handleLoggingMethodAsync(
  request: ExtensionRequest,
  extension: IHaexSpaceExtension,
) {
  if (!extension || !request) {
    throw new Error('Extension not found')
  }

  const { method, params } = request
  const deviceStore = useDeviceStore()

  switch (method) {
    case 'extension_logging_write': {
      await invoke('extension_logging_write', {
        level: params.level as string,
        extensionId: extension.id,
        message: params.message as string,
        metadata: params.metadata ?? null,
        deviceId: deviceStore.deviceId ?? 'unknown',
      })
      return
    }

    case 'extension_logging_read': {
      return await invoke('extension_logging_read', {
        extensionId: extension.id,
        query: {
          level: (params.level as string) ?? null,
          limit: (params.limit as number) ?? null,
          offset: (params.offset as number) ?? null,
        },
      })
    }

    default:
      throw new Error(`Unknown logging method: ${method}`)
  }
}
