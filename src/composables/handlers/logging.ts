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
      // Identity is resolved server-side from publicKey/name (iframe) or the
      // window (WebView) — see extension_logging_write in Rust. We no longer
      // pass a raw extensionId, which could be spoofed.
      await invoke('extension_logging_write', {
        level: params.level as string,
        publicKey: extension.publicKey,
        name: extension.name,
        message: params.message as string,
        metadata: params.metadata ?? null,
        deviceId: deviceStore.deviceId ?? 'unknown',
      })
      return
    }

    case 'extension_logging_read': {
      return await invoke('extension_logging_read', {
        publicKey: extension.publicKey,
        name: extension.name,
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
