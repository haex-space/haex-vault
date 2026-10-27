import { reactive } from 'vue'
import { ALL_ACTIONS } from '~/lib/actions/catalog'
import {
  createHandlerRegistry,
  type ActionHandler,
} from '~/lib/actions/handlers'
import { tabCandidates } from '~/lib/actions/handlers'
import { createActionRunner } from '~/lib/actions/runner'
import type { ActionCaller, ActionOutcome } from '~/lib/actions/types'
import type { TabHistory, TabLocation } from '~/lib/wm/navigation'
import {
  goTab,
  navigateTab,
  resolveSystemBack,
  setTabHistory,
  skipTabEntry,
  syncHistories,
  type TabHistories,
} from '~/lib/wm/tabNavigation'
import type { WmState } from '~/lib/wm/types'

/**
 * Per-tab navigation and the action runner, wired into the window manager store
 * (spec 020-tab-navigation, T015). Kept out of `stores/windowManager.ts` so that file
 * stays below the 500-line limit; the logic itself lives in the pure modules
 * under `lib/wm/tabNavigation.ts` and `lib/actions/` (tested by
 * `pnpm check:wm-navigation`).
 *
 * Histories are a reactive `Map` next to the store's tab runtime — never
 * persisted (FR-011), dropped with their tab, and untouched by window or
 * workspace operations (FR-010).
 */
export function createWmNavigation(deps: {
  state: WmState
  /** The title override a tab shows right now (stored on the entry it leaves, research R9). */
  titleOverrideOf: (tabId: string) => string | null
  /** Navigation resets a dynamic title; the app sets it again for the new view (research R9). */
  clearTitleOverride: (tabId: string) => void
  /** The store's `openApp` (persists); returns the app's tab id. */
  openApp: (appId: string) => string | null
}) {
  const { state } = deps
  const histories: TabHistories = reactive(new Map<string, TabHistory>())
  const handlers = createHandlerRegistry()
  /** Last travel direction per tab, for skipping a stale entry the same way (FR-027). */
  const lastDirection = new Map<string, -1 | 1>()

  /** The window manager's own overlays, here rather than local to `wm/Desktop.vue` so system back can
   * close them and open the window overview (FR-019). Dropdown menus close themselves. */
  const overlays = reactive({
    launcher: false,
    windows: false,
    workspaces: false,
  })

  /** `wm.system.back` (contracts/wm-actions.md §5). */
  function systemBack(): string {
    const overlayOpen =
      overlays.launcher || overlays.windows || overlays.workspaces
    const decision = resolveSystemBack(state, histories, overlayOpen)
    if (decision.kind === 'closeOverlay') {
      overlays.launcher = false
      overlays.windows = false
      overlays.workspaces = false
    } else if (decision.kind === 'back') go(decision.tabId, -1)
    else if (decision.kind === 'openWindowOverview') overlays.windows = true
    return decision.kind
  }

  function syncNavigation() {
    const before = new Set(histories.keys())
    syncHistories(histories, state)
    for (const id of before) {
      if (!histories.has(id)) handlers.dropTab(id)
    }
  }

  function historyOf(tabId: string): TabHistory | undefined {
    return histories.get(tabId)
  }

  function navigate(
    tabId: string,
    to: string | TabLocation,
    options: { replace?: boolean } = {},
  ): boolean {
    const changed = navigateTab(histories, tabId, to, {
      replace: options.replace,
      leavingTitle: deps.titleOverrideOf(tabId),
    })
    if (changed && !options.replace) {
      lastDirection.set(tabId, 1)
      deps.clearTitleOverride(tabId)
    }
    return changed
  }

  function go(tabId: string, delta: number): boolean {
    const changed = goTab(histories, tabId, delta, deps.titleOverrideOf(tabId))
    if (changed) {
      lastDirection.set(tabId, delta < 0 ? -1 : 1)
      deps.clearTitleOverride(tabId)
    }
    return changed
  }

  /** Replaces a tab's whole history (spec 062: the file browser swaps the history of its active
   * panel); `false` and no change for a tab without history or a history that breaks its
   * invariants. The last travel direction and the dynamic title belong to the old history. */
  function setHistory(tabId: string, history: TabHistory): boolean {
    const changed = setTabHistory(histories, tabId, history)
    if (changed) {
      lastDirection.delete(tabId)
      deps.clearTitleOverride(tabId)
    }
    return changed
  }

  /** Drops the tab's current entry (its target is gone) and moves on in the last travel
   * direction; `false` if it was the only entry. */
  function skipCurrent(tabId: string): boolean {
    return skipTabEntry(histories, tabId, lastDirection.get(tabId) ?? -1)
  }

  function tabIdsOf(appId: string): string[] {
    return state.windows.flatMap((w) =>
      w.tabs.filter((t) => t.appId === appId).map((t) => t.id),
    )
  }

  const runner = createActionRunner({
    catalog: ALL_ACTIONS,
    globalHandler: (id) => handlers.globalHandler(id),
    resolveFocus: (target) => {
      if (target === 'workspace') return state.activeWorkspaceId || null
      const window = state.windows.find((w) => w.id === state.activeWindowId)
      if (!window) return null
      return target === 'window' ? window.id : window.activeTabId
    },
    targetExists: (target, id) => {
      if (target === 'workspace')
        return state.workspaces.some((w) => w.id === id)
      if (target === 'window') return state.windows.some((w) => w.id === id)
      return state.windows.some((w) => w.tabs.some((t) => t.id === id))
    },
    awaitTabHandler: (appId, actionId, timeoutMs, originTabId) => {
      // UI actions carry their originating tab so a second instance of the same app cannot answer
      // on its behalf. Calls without an origin keep the global app activation behavior for agents
      // and other non-tab UI callers.
      const active = state.windows.find((w) => w.id === state.activeWindowId)
      const activeTab = active?.tabs.find((t) => t.id === active.activeTabId)
      const candidateTabIds = originTabId
        ? [originTabId]
        : tabCandidates(appId, activeTab, tabIdsOf(appId))
      const ready = handlers.findTab(candidateTabIds, actionId)
      if (ready) return Promise.resolve(ready)
      if (!originTabId) deps.openApp(appId)
      return handlers.awaitTab(candidateTabIds, actionId, timeoutMs)
    },
  })

  function runAction(
    id: string,
    input: Record<string, unknown> = {},
    caller: ActionCaller = { kind: 'user' },
    originTabId?: string,
  ): Promise<ActionOutcome> {
    return runner.runAction(id, input, caller, originTabId)
  }

  function registerGlobalActionHandler(id: string, handler: ActionHandler) {
    handlers.registerGlobal(id, handler)
  }

  function registerTabActionHandler(
    tabId: string,
    id: string,
    handler: ActionHandler,
  ): () => void {
    return handlers.registerTab(tabId, id, handler)
  }

  return {
    histories,
    overlays,
    systemBack,
    syncNavigation,
    historyOf,
    navigate,
    go,
    setHistory,
    skipCurrent,
    runAction,
    registerGlobalActionHandler,
    registerTabActionHandler,
  }
}
