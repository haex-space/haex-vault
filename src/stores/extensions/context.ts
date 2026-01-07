/**
 * Extension Context Store
 *
 * Centralized store for managing the application context that is shared with extensions.
 * This store maintains the current context and handles broadcasting to extensions.
 *
 * The context includes:
 * - theme: Current UI theme (light/dark/system)
 * - locale: Current language/locale
 * - platform: Operating system (android/ios/macos/windows/linux)
 * - deviceId: Unique device identifier
 *
 * Additional context properties can be added here as needed.
 */

import { invoke } from '@tauri-apps/api/core'
import { TAURI_COMMANDS, type ApplicationContext } from '@haex-space/vault-sdk'
import { useExtensionBroadcastStore } from './broadcast'

export const useExtensionContextStore = defineStore('extensionContextStore', () => {
  // Current context - cached so we can send it to newly registered iframes
  const context = ref<ApplicationContext | null>(null)

  // Get dependencies
  const deviceStore = useDeviceStore()
  const uiStore = useUiStore()
  const { currentTheme } = storeToRefs(uiStore)
  const { locale } = useI18n()
  const { deviceId } = storeToRefs(deviceStore)

  /**
   * Build the current context from all sources
   */
  const buildContext = (): ApplicationContext => {
    return {
      theme: (currentTheme.value?.value || 'dark') as 'light' | 'dark' | 'system',
      locale: locale.value,
      platform: deviceStore.platform,
      deviceId: deviceStore.deviceId,
    }
  }

  /**
   * Update and broadcast the context to all extensions
   */
  const updateContext = async () => {
    const newContext = buildContext()
    context.value = newContext

    console.log('[ExtensionContext] Context updated:', newContext)

    // Store context in Tauri state (for webview extensions to query on init)
    try {
      await invoke(TAURI_COMMANDS.extension.setContext, { context: newContext })
      console.log('[ExtensionContext] Context stored in Tauri state')
    } catch (error) {
      // Log error - could be browser mode, Android, or Tauri not ready
      console.error('[ExtensionContext] Failed to store context in Tauri:', error)
    }

    // Broadcast to all extensions (iframes + webviews)
    const broadcastStore = useExtensionBroadcastStore()
    await broadcastStore.broadcastContext(newContext)
  }

  /**
   * Get the current context (for sending to newly registered iframes)
   */
  const getContext = (): ApplicationContext | null => {
    return context.value
  }

  // Watch for changes in theme, locale, or deviceId and update context
  watch(
    [currentTheme, locale, deviceId],
    () => {
      console.log('[ExtensionContext] Dependency changed, updating context')
      updateContext()
    },
    { immediate: true },
  )

  return {
    context: readonly(context),
    getContext,
    updateContext,
  }
})
