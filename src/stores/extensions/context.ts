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
import { isDesktop } from '~/utils/platform'

export const useExtensionContextStore = defineStore('extensionContextStore', () => {
  // Current context - cached so we can send it to newly registered iframes
  const context = ref<ApplicationContext | null>(null)

  // Get dependencies
  const deviceStore = useDeviceStore()
  const uiStore = useUiStore()
  const { currentThemeName } = storeToRefs(uiStore)
  const { deviceId } = storeToRefs(deviceStore)

  // Use global i18n instance from Nuxt app for proper reactivity
  // useI18n() in a Pinia store may not be reactive to global locale changes
  const { $i18n } = useNuxtApp()
  const locale = computed(() => $i18n.locale.value)

  /**
   * Build the current context from all sources
   */
  const buildContext = (): ApplicationContext => {
    return {
      theme: (currentThemeName.value || 'dark') as 'light' | 'dark' | 'system',
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
    // This is only needed on Desktop where WebView extensions use Tauri invoke
    // On mobile, extensions use iframes with postMessage and get context via broadcast
    if (isDesktop()) {
      try {
        await invoke(TAURI_COMMANDS.extension.setContext, { context: newContext })
        console.log('[ExtensionContext] Context stored in Tauri state')
      } catch (error) {
        console.error('[ExtensionContext] Failed to store context in Tauri:', error)
      }
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
  // Note: We watch currentThemeName (string ref) instead of currentTheme (computed object)
  // because watching a computed that returns an object from a static array won't trigger
  // when the underlying value changes (same object reference)
  watch(
    [currentThemeName, locale, deviceId],
    () => {
      console.log('[ExtensionContext] Dependency changed, updating context')
      // Call async function with proper error handling to avoid unhandled rejections
      updateContext().catch((error) => {
        console.error('[ExtensionContext] Failed to update context:', error)
      })
    },
    { immediate: true },
  )

  return {
    context: readonly(context),
    getContext,
    updateContext,
  }
})
