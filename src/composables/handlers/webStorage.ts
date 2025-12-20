import { TAURI_COMMANDS } from '@haex-space/vault-sdk'
import type { ExtensionRequest, ExtensionInstance } from './types'

export async function handleWebStorageMethodAsync(
  request: ExtensionRequest,
  instance: ExtensionInstance,
) {
  // Storage is now per-window, not per-extension
  const storageKey = `ext_${instance.extension.id}_${instance.windowId}_`

  switch (request.method) {
    case TAURI_COMMANDS.webStorage.getItem: {
      const key = request.params.key as string
      return localStorage.getItem(storageKey + key)
    }

    case TAURI_COMMANDS.webStorage.setItem: {
      const key = request.params.key as string
      const value = request.params.value as string
      localStorage.setItem(storageKey + key, value)
      return null
    }

    case TAURI_COMMANDS.webStorage.removeItem: {
      const key = request.params.key as string
      localStorage.removeItem(storageKey + key)
      return null
    }

    case TAURI_COMMANDS.webStorage.clear: {
      // Remove only instance-specific keys
      const keys = Object.keys(localStorage).filter(k =>
        k.startsWith(storageKey),
      )
      keys.forEach(k => localStorage.removeItem(k))
      return null
    }

    case TAURI_COMMANDS.webStorage.keys: {
      // Return only instance-specific keys (without prefix)
      const keys = Object.keys(localStorage)
        .filter(k => k.startsWith(storageKey))
        .map(k => k.substring(storageKey.length))
      return keys
    }

    default:
      throw new Error(`Unknown web storage method: ${request.method}`)
  }
}
