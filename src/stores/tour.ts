import { driver } from 'driver.js'
import type { Driver } from 'driver.js'
import de from './tour.de.json'
import en from './tour.en.json'

const STORAGE_KEY = 'haex-tour-completed'

export const useTourStore = defineStore('tourStore', () => {
  const { t } = useI18n()
  const { $i18n } = useNuxtApp()
  $i18n.mergeLocaleMessage('de', { tour: de })
  $i18n.mergeLocaleMessage('en', { tour: en })

  const windowManager = useWindowManagerStore()

  const isCompleted = ref(localStorage.getItem(STORAGE_KEY) === 'true')
  let driverInstance: Driver | null = null

  const navigateTo = async (category: string) => {
    await windowManager.openWindowAsync({
      type: 'system',
      sourceId: 'settings',
      params: { category },
    })
    // Two ticks: one for Vue reactivity, one for DOM render
    await nextTick()
    await nextTick()
  }

  const complete = () => {
    localStorage.setItem(STORAGE_KEY, 'true')
    isCompleted.value = true
    driverInstance?.destroy()
    driverInstance = null
  }

  const start = async () => {
    if (isCompleted.value) return

    // Open settings at general tab first, wait for DOM
    await navigateTo('general')

    driverInstance = driver({
      animate: true,
      overlayColor: 'rgba(0,0,0,0.6)',
      allowClose: false,
      stagePadding: 6,
      popoverClass: 'haex-tour-popover',
      nextBtnText: t('tour.next'),
      prevBtnText: t('tour.prev'),
      doneBtnText: t('tour.done'),
      onDestroyStarted: () => {
        complete()
        driverInstance?.destroy()
      },
      steps: [
        {
          element: '[data-tour="settings-nav-general"]',
          popover: {
            title: t('tour.steps.general.title'),
            description: t('tour.steps.general.description'),
          },
        },
        {
          element: '[data-tour="settings-device-name"]',
          popover: {
            title: t('tour.steps.deviceName.title'),
            description: t('tour.steps.deviceName.description'),
            onNextClick: async () => {
              await navigateTo('extensions')
              driverInstance?.moveNext()
            },
          },
        },
        {
          element: '[data-tour="settings-nav-extensions"]',
          popover: {
            title: t('tour.steps.extensionsNav.title'),
            description: t('tour.steps.extensionsNav.description'),
          },
        },
        {
          element: '[data-tour="settings-extensions-install"]',
          popover: {
            title: t('tour.steps.extensions.title'),
            description: t('tour.steps.extensions.description'),
            onNextClick: async () => {
              await navigateTo('identities')
              driverInstance?.moveNext()
            },
          },
        },
        {
          element: '[data-tour="settings-nav-identities"]',
          popover: {
            title: t('tour.steps.identitiesNav.title'),
            description: t('tour.steps.identitiesNav.description'),
          },
        },
        {
          element: '[data-tour="settings-identities-create"]',
          popover: {
            title: t('tour.steps.identity.title'),
            description: t('tour.steps.identity.description'),
            onNextClick: async () => {
              await navigateTo('sync')
              driverInstance?.moveNext()
            },
          },
        },
        {
          element: '[data-tour="settings-nav-sync"]',
          popover: {
            title: t('tour.steps.syncNav.title'),
            description: t('tour.steps.syncNav.description'),
          },
        },
        {
          element: '[data-tour="settings-sync-add-backend"]',
          popover: {
            title: t('tour.steps.sync.title'),
            description: t('tour.steps.sync.description'),
          },
        },
      ],
    })

    driverInstance.drive()
  }

  return { isCompleted, start, complete }
})
