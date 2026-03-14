import { driver } from 'driver.js'
import type { Driver } from 'driver.js'
import de from './tour.de.json'
import en from './tour.en.json'

type TourMessages = typeof de

function getNestedValue(obj: Record<string, unknown>, path: string): string {
  return path.split('.').reduce((acc: unknown, key) => {
    return acc && typeof acc === 'object' ? (acc as Record<string, unknown>)[key] : undefined
  }, obj) as string ?? path
}

export const useTourStore = defineStore('tourStore', () => {
  const { $i18n } = useNuxtApp()
  const windowManager = useWindowManagerStore()
  const launcherStore = useLauncherStore()

  const isActive = ref(false)
  let driverInstance: Driver | null = null

  const t = (key: string): string => {
    const locale = $i18n.locale.value as 'de' | 'en'
    const messages: TourMessages = locale === 'de' ? de : en
    return getNestedValue(messages as unknown as Record<string, unknown>, key)
  }

  const navigateSettings = async (category: string) => {
    await windowManager.openWindowAsync({
      type: 'system',
      sourceId: 'settings',
      params: { category },
    })
    await nextTick()
    await nextTick()
  }

  const complete = () => {
    isActive.value = false
    driverInstance?.destroy()
    driverInstance = null
  }

  const start = async () => {
    if (isActive.value) return

    isActive.value = true

    driverInstance = driver({
      animate: true,
      overlayColor: 'rgba(0,0,0,0.6)',
      allowClose: true,
      stagePadding: 6,
      popoverClass: 'haex-tour-popover',
      nextBtnText: t('next'),
      prevBtnText: t('prev'),
      doneBtnText: t('done'),
      onDestroyStarted: () => {
        complete()
        driverInstance?.destroy()
      },
      steps: [
        {
          element: '[data-testid="launcher-button"]',
          popover: {
            title: t('steps.launcher.title'),
            description: t('steps.launcher.description'),
            onNextClick: async () => {
              launcherStore.isOpen = true
              await new Promise(resolve => setTimeout(resolve, 300))
              driverInstance?.moveNext()
            },
          },
        },
        {
          element: '[data-tour="launcher-settings-item"]',
          popover: {
            title: t('steps.launcherSettings.title'),
            description: t('steps.launcherSettings.description'),
            onNextClick: async () => {
              launcherStore.isOpen = false
              await navigateSettings('general')
              driverInstance?.moveNext()
            },
          },
        },
        {
          element: '[data-tour="settings-nav-general"]',
          popover: {
            title: t('steps.general.title'),
            description: t('steps.general.description'),
            onNextClick: async () => {
              await navigateSettings('devices')
              driverInstance?.moveNext()
            },
          },
        },
        {
          element: '[data-tour="settings-device-name"]',
          popover: {
            title: t('steps.deviceName.title'),
            description: t('steps.deviceName.description'),
            onNextClick: async () => {
              await navigateSettings('extensions')
              driverInstance?.moveNext()
            },
          },
        },
        {
          element: '[data-tour="settings-nav-extensions"]',
          popover: {
            title: t('steps.extensionsNav.title'),
            description: t('steps.extensionsNav.description'),
          },
        },
        {
          element: '[data-tour="settings-extensions-install"]',
          popover: {
            title: t('steps.extensions.title'),
            description: t('steps.extensions.description'),
            onNextClick: async () => {
              await navigateSettings('identities')
              driverInstance?.moveNext()
            },
          },
        },
        {
          element: '[data-tour="settings-nav-identities"]',
          popover: {
            title: t('steps.identitiesNav.title'),
            description: t('steps.identitiesNav.description'),
          },
        },
        {
          element: '[data-tour="settings-identities-create"]',
          popover: {
            title: t('steps.identity.title'),
            description: t('steps.identity.description'),
            onNextClick: async () => {
              await navigateSettings('sync')
              driverInstance?.moveNext()
            },
          },
        },
        {
          element: '[data-tour="settings-nav-sync"]',
          popover: {
            title: t('steps.syncNav.title'),
            description: t('steps.syncNav.description'),
          },
        },
        {
          element: '[data-tour="settings-sync-add-backend"]',
          popover: {
            title: t('steps.sync.title'),
            description: t('steps.sync.description'),
          },
        },
      ],
    })

    driverInstance.drive()
  }

  return { isActive, start, complete }
})
