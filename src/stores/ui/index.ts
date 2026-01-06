import { breakpointsTailwind } from '@vueuse/core'
import { invoke } from '@tauri-apps/api/core'
import { HAEXTENSION_EVENTS, TAURI_COMMANDS } from '@haex-space/vault-sdk'
import { broadcastContextToAllExtensions } from '~/composables/extensionMessageHandler'

import de from './de.json'
import en from './en.json'

export const useUiStore = defineStore('uiStore', () => {
  const breakpoints = useBreakpoints(breakpointsTailwind)

  // "mdAndDown" gilt für md, sm, xs usw.
  const isSmallScreen = breakpoints.smaller('md')

  const { $i18n } = useNuxtApp()
  const { locale } = useI18n({
    useScope: 'global',
  })

  $i18n.setLocaleMessage('de', {
    ui: de,
  })
  $i18n.setLocaleMessage('en', { ui: en })

  const availableThemes = ref([
    {
      value: 'dark',
      label: $i18n.t('ui.dark'),
      icon: 'line-md:moon-rising-alt-loop',
    },
    {
      value: 'light',
      label: $i18n.t('ui.light'),
      icon: 'line-md:moon-to-sunny-outline-loop-transition',
    },
    /*     {
      value: 'soft',
      label: t('ui.soft'),
      icon: 'line-md:paint-drop',
    },

    {
      value: 'corporate',
      label: t('ui.corporate'),
      icon: 'hugeicons:corporate',
    }, */
  ])

  const defaultTheme = ref('dark')

  const currentThemeName = ref(defaultTheme.value)

  const currentTheme = computed(
    () =>
      availableThemes.value.find(
        (theme) => theme.value === currentThemeName.value,
      ) ?? availableThemes.value.at(0),
  )

  const colorMode = useColorMode()

  watchImmediate(currentThemeName, () => {
    colorMode.preference = currentThemeName.value
  })

  // Broadcast theme and locale changes to extensions (including initial state)
  // Also watch deviceId to update context when it becomes available
  const deviceStore = useDeviceStore()
  const { deviceId } = storeToRefs(deviceStore)

  watch([currentThemeName, locale, deviceId], async () => {
    const context = {
      theme: currentThemeName.value,
      locale: locale.value,
      platform: deviceStore.platform,
      deviceId: deviceStore.deviceId,
    }

    console.log('[UI Store] Watch triggered - broadcasting context:', context)

    // Broadcast to iframe extensions (existing)
    broadcastContextToAllExtensions(context)

    // Update Tauri state and emit event for webview extensions
    try {
      console.log('[UI Store] Calling extension_context_set...')
      await invoke(TAURI_COMMANDS.extension.setContext, { context })
      console.log('[UI Store] Context set in Tauri state:', context)
      // Broadcast event to all webview extensions
      console.log('[UI Store] Calling extension_emit_to_all...')
      await invoke(TAURI_COMMANDS.extension.emitToAll, {
        event: HAEXTENSION_EVENTS.CONTEXT_CHANGED,
        payload: { context }
      })
      console.log('[UI Store] Broadcasted context change event to webview extensions:', context)
    } catch (error) {
      // Log error - could be browser mode or Tauri not ready
      console.error('[UI Store] Failed to update Tauri context:', error)
    }
  }, { immediate: true })

  const viewportHeightWithoutHeader = ref(0)
  const headerHeight = ref(0)

  return {
    availableThemes,
    viewportHeightWithoutHeader,
    headerHeight,
    currentTheme,
    currentThemeName,
    defaultTheme,
    isSmallScreen,
  }
})
