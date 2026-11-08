import type { Translate } from '~/composables/useModelInventory'
import { ActionChoiceError, ActionInputError } from '~/lib/actions/runner'
import { matchApp } from '~/lib/wm/appMatch'
import type { useWindowManagerStore } from '~/stores/windowManager'

type WmStore = ReturnType<typeof useWindowManagerStore>

/**
 * Global handlers of the window manager actions (spec 020-tab-navigation,
 * contracts/wm-actions.md §1), registered once at startup by
 * `plugins/actions.client.ts`. The runner has already validated the input
 * and resolved the target, so handlers only perform the change and describe
 * the result.
 */
export function registerWmActionHandlers(wm: WmStore, t: Translate): void {
  wm.registerGlobalActionHandler('wm.tab.back', ({ target }) => ({
    moved: wm.goTab(target.tabId ?? '', -1),
  }))
  wm.registerGlobalActionHandler('wm.tab.forward', ({ target }) => ({
    moved: wm.goTab(target.tabId ?? '', 1),
  }))
  wm.registerGlobalActionHandler('wm.tab.go', ({ target, input }) => ({
    moved: wm.goTab(target.tabId ?? '', Number(input.steps)),
  }))
  wm.registerGlobalActionHandler('wm.tab.navigate', ({ target, input }) => ({
    moved: wm.navigate(target.tabId ?? '', String(input.to), {
      replace: input.replace === true,
    }),
  }))
  wm.registerGlobalActionHandler('wm.system.back', () => ({
    outcome: wm.systemBack(),
  }))

  /** The app and start location to open: `appId` may be an id, a replaced id (spec 023 research
   * R11) or a name with typos (spec 046, FR-001); an `at` from the input wins over the alias's. An
   * input that names no app clearly asks to choose (FR-004): `openApp` itself silently ignores
   * unknown ids. */
  function target(input: Record<string, unknown>): {
    appId: string
    at: string | null
  } {
    const requested = String(input.appId)
    const apps = wm.apps()
    const match = matchApp(
      requested,
      apps,
      (app) => app.title ?? t(app.titleKey),
    )
    if (match.kind === 'website') {
      // A model that is told to "open serienstream.to" reaches for the app tool; say where it goes.
      const address = /^https?:\/\//i.test(match.address)
        ? match.address
        : `https://${match.address}`
      throw new ActionInputError(
        `${requested} is a website, not an app. Open it in the browser with browser_open (url: ${address}).`,
        'appId',
      )
    }
    if (match.kind === 'choice') {
      throw new ActionChoiceError(
        `no app matches ${requested} unambiguously`,
        'appId',
        match.candidates.map((candidate) => ({
          value: candidate.appId,
          label: candidate.title,
          ...(candidate.unavailableKey
            ? { unavailable: t(candidate.unavailableKey) }
            : {}),
        })),
      )
    }
    const app = apps.find((candidate) => candidate.id === match.appId)
    if (app?.unavailableKey) {
      throw new ActionChoiceError(
        `${requested} is not available on this device`,
        'appId',
        [
          {
            value: app.id,
            label: app.title ?? t(app.titleKey),
            unavailable: t(app.unavailableKey),
          },
        ],
      )
    }
    return {
      appId: match.appId,
      at: typeof input.at === 'string' ? input.at : match.at,
    }
  }

  wm.registerGlobalActionHandler('wm.app.open', ({ input }) => {
    const { appId, at } = target(input)
    const before = new Set(wm.windows.flatMap((w) => w.tabs.map((t) => t.id)))
    const tabId = wm.openApp(appId, at)
    return { tabId, created: tabId !== null && !before.has(tabId) }
  })
  wm.registerGlobalActionHandler('wm.tab.new', ({ input, target: where }) => {
    const { appId, at } = target(input)
    const before = new Set(wm.windows.flatMap((w) => w.tabs.map((t) => t.id)))
    const tabId = wm.addTab(where.windowId ?? '', appId, at)
    return { tabId, created: tabId !== null && !before.has(tabId) }
  })
}
