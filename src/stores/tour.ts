import { SettingsCategory } from '~/config/settingsCategories'
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

/** Resolve once `selector` exists in the DOM, or after `timeoutMs` (so the
 *  tour never wedges if a capability-gated control never appears). */
function waitForElement(selector: string, timeoutMs = 4000): Promise<void> {
  return new Promise((resolve) => {
    const start = Date.now()
    const check = () => {
      if (document.querySelector(selector) || Date.now() - start > timeoutMs) {
        resolve()
        return
      }
      requestAnimationFrame(check)
    }
    check()
  })
}

export const useTourStore = defineStore('tourStore', () => {
  const { $i18n } = useNuxtApp()
  const windowManager = useWindowManagerStore()
  const launcherStore = useLauncherStore()

  const isActive = ref(false)
  let driverInstance: Driver | null = null
  let completeResolver: (() => void) | null = null
  // Track the active tour's completion promise so concurrent start() callers
  // all await the same end-of-tour signal. Without this, a second start()
  // while a tour is running would return Promise.resolve() and break the
  // "await start() means the tour is finished" contract.
  let activeTourPromise: Promise<void> | null = null

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
    completeResolver?.()
    completeResolver = null
    activeTourPromise = null
  }

  const start = (): Promise<void> => {
    if (isActive.value) return activeTourPromise ?? Promise.resolve()

    isActive.value = true

    const promise = new Promise<void>((resolve) => {
      completeResolver = resolve

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
            title: t('steps.settings.title'),
            description: t('steps.settings.description'),
            onNextClick: async () => {
              launcherStore.isOpen = false
              await navigateSettings(SettingsCategory.Extensions)
              driverInstance?.moveNext()
            },
          },
        },
        {
          element: '[data-tour="settings-nav-extensions"]',
          popover: {
            title: t('steps.extensions.title'),
            description: t('steps.extensions.description'),
            onNextClick: async () => {
              await navigateSettings(SettingsCategory.Spaces)
              await waitForElement('[data-tour="settings-spaces-create"]')
              driverInstance?.moveNext()
            },
          },
        },
        {
          element: '[data-tour="settings-spaces-create"]',
          popover: {
            title: t('steps.spacesOverview.title'),
            description: t('steps.spacesOverview.description'),
            onNextClick: async () => {
              // Invite/add-share buttons render only after the card resolves
              // its capabilities from the UCAN store.
              await waitForElement('[data-tour="space-invite"]')
              driverInstance?.moveNext()
            },
          },
        },
        {
          element: '[data-tour="space-invite"]',
          popover: {
            title: t('steps.spacesInvite.title'),
            description: t('steps.spacesInvite.description'),
            onNextClick: async () => {
              await waitForElement('[data-tour="space-add-share"]')
              driverInstance?.moveNext()
            },
          },
        },
        {
          element: '[data-tour="space-add-share"]',
          popover: {
            title: t('steps.spacesShare.title'),
            description: t('steps.spacesShare.description'),
            onNextClick: async () => {
              await navigateSettings(SettingsCategory.Sync)
              await waitForElement('[data-tour="settings-nav-sync"]')
              driverInstance?.moveNext()
            },
          },
        },
        {
          element: '[data-tour="settings-nav-sync"]',
          popover: {
            title: t('steps.sync.title'),
            description: t('steps.sync.description'),
          },
        },
      ],
    })

      driverInstance.drive()
    })

    activeTourPromise = promise
    return promise
  }

  return { isActive, start, complete }
})
