import { breakpointsTailwind } from '@vueuse/core'
import { invoke } from '@tauri-apps/api/core'
import { TAURI_COMMANDS } from '@haex-space/vault-sdk'
import { useExtensionBroadcastStore } from '~/stores/extensions/broadcast'

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
  const broadcastStore = useExtensionBroadcastStore()

  watch([currentThemeName, locale, deviceId], async () => {
    const context = {
      theme: currentThemeName.value as 'light' | 'dark' | 'system',
      locale: locale.value,
      platform: deviceStore.platform,
      deviceId: deviceStore.deviceId,
    }

    console.log('[UI Store] Watch triggered - broadcasting context:', context)

    // Store context in Tauri state (for webview extensions to query on init)
    try {
      await invoke(TAURI_COMMANDS.extension.setContext, { context })
      console.log('[UI Store] Context stored in Tauri state')
    } catch (error) {
      // Log error - could be browser mode or Tauri not ready
      console.error('[UI Store] Failed to store context:', error)
    }

    // Broadcast to all extensions (iframes + webviews)
    await broadcastStore.broadcastContext(context)
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
