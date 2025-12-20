import type { Platform } from '@tauri-apps/plugin-os'
import { TAURI_COMMANDS } from '@haex-space/vault-sdk'
import type { ExtensionRequest } from './types'

// Context getters are set from the main handler during initialization
let contextGetters: {
  getTheme: () => string
  getLocale: () => string
  getPlatform: () => Platform | undefined
  getDeviceId: () => string | undefined
} | null = null

export function setContextGetters(getters: {
  getTheme: () => string
  getLocale: () => string
  getPlatform: () => Platform | undefined
  getDeviceId: () => string | undefined
}) {
  contextGetters = getters
}

export async function handleContextMethodAsync(request: ExtensionRequest) {
  switch (request.method) {
    case TAURI_COMMANDS.extension.getContext:
      if (!contextGetters) {
        throw new Error(
          'Context not initialized. Make sure useExtensionMessageHandler is called in a component.',
        )
      }
      return {
        theme: contextGetters.getTheme(),
        locale: contextGetters.getLocale(),
        platform: contextGetters.getPlatform(),
        deviceId: contextGetters.getDeviceId(),
      }

    default:
      throw new Error(`Unknown context method: ${request.method}`)
  }
}
