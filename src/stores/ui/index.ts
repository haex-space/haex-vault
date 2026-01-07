import { breakpointsTailwind } from '@vueuse/core'

import de from './de.json'
import en from './en.json'

export const useUiStore = defineStore('uiStore', () => {
  const breakpoints = useBreakpoints(breakpointsTailwind)

  // "mdAndDown" gilt für md, sm, xs usw.
  const isSmallScreen = breakpoints.smaller('md')

  const { $i18n } = useNuxtApp()

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
