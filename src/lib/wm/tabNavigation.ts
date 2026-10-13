// Store-level tab navigation (spec 020-tab-navigation, T014, research R2/R11/R12): keeps one
// history per open tab in sync with the layout and opens apps or tabs at a location. The layout
// reducers stay unchanged; these helpers run them and then tell a new tab (fresh history at the
// location) from a re-activated singleton (push the location onto its history). Histories live
// next to the store's non-persisted tab runtime, never in the persisted layout (FR-011).
import type { AppDefinition } from './apps.ts'
import {
  findOpenTab,
  frontmostWindow,
  openApp,
  openAppInBackground,
} from './layoutState.ts'
import {
  canGoBack,
  createHistory,
  go,
  push,
  removeEntry,
  replace,
  type TabHistory,
  type TabLocation,
} from './navigation.ts'
import { addTab } from './tabs.ts'
import type { WmState } from './types.ts'

export type TabHistories = Map<string, TabHistory>

/** Title the tab shows right now, stored on the entry it leaves (research R9). */
export type LeavingTitle = (tabId: string) => string | null

export type OpenResult = { tabId: string | null; created: boolean }

function tabIds(state: WmState): Set<string> {
  return new Set(state.windows.flatMap((w) => w.tabs.map((t) => t.id)))
}

/** Drops histories of closed tabs and gives every new tab a fresh `/` history. */
export function syncHistories(histories: TabHistories, state: WmState): void {
  const live = tabIds(state)
  for (const id of [...histories.keys()]) {
    if (!live.has(id)) histories.delete(id)
  }
  for (const id of live) {
    if (!histories.has(id)) histories.set(id, createHistory())
  }
}

function settle(
  state: WmState,
  histories: TabHistories,
  before: Set<string>,
  appId: string,
  at: string | TabLocation | null,
  leavingTitle: LeavingTitle,
): OpenResult {
  const created = [...tabIds(state)].find((id) => !before.has(id))
  if (created) {
    histories.set(created, createHistory(at ?? '/'))
    syncHistories(histories, state)
    return { tabId: created, created: true }
  }
  const existing = findOpenTab(state, appId)?.tab.id ?? null
  syncHistories(histories, state)
  if (existing && at !== null) {
    const history = histories.get(existing)
    if (history)
      histories.set(existing, push(history, at, leavingTitle(existing)))
  }
  return { tabId: existing, created: false }
}

/** `openApp` (015 FR-016) plus a start location (FR-012). */
export function openAppAt(
  state: WmState,
  histories: TabHistories,
  appId: string,
  at: string | TabLocation | null,
  apps: readonly AppDefinition[],
  leavingTitle: LeavingTitle = () => null,
): OpenResult {
  const before = tabIds(state)
  openApp(state, appId, apps)
  return settle(state, histories, before, appId, at, leavingTitle)
}

/** `openAppInBackground` (spec 018, research R9): a new tab starts at `/`, an open one keeps its
 * history. */
export function openAppInBackgroundAt(
  state: WmState,
  histories: TabHistories,
  appId: string,
  apps: readonly AppDefinition[],
): OpenResult {
  const before = tabIds(state)
  const tabId = openAppInBackground(state, appId, apps)
  syncHistories(histories, state)
  return { tabId, created: tabId !== null && !before.has(tabId) }
}

/** `addTab` (015 FR-032/FR-033) plus a start location (FR-012). */
export function addTabAt(
  state: WmState,
  histories: TabHistories,
  windowId: string,
  appId: string,
  at: string | TabLocation | null,
  apps: readonly AppDefinition[],
  leavingTitle: LeavingTitle = () => null,
): OpenResult {
  const before = tabIds(state)
  addTab(state, windowId, appId, apps)
  return settle(state, histories, before, appId, at, leavingTitle)
}

/** Push (FR-004) or replace (FR-005) on one tab's history; `true` if it changed. */
export function navigateTab(
  histories: TabHistories,
  tabId: string,
  to: string | TabLocation,
  options: { replace?: boolean; leavingTitle?: string | null } = {},
): boolean {
  const history = histories.get(tabId)
  if (!history) return false
  const next = options.replace
    ? replace(history, to)
    : push(history, to, options.leavingTitle ?? null)
  if (next === history) return false
  histories.set(tabId, next)
  return true
}

/** Back/forward/jump on one tab's history (FR-007); `true` if it moved. */
export function goTab(
  histories: TabHistories,
  tabId: string,
  delta: number,
  leavingTitle: string | null = null,
): boolean {
  const history = histories.get(tabId)
  if (!history) return false
  const next = go(history, delta, leavingTitle)
  if (next === history) return false
  histories.set(tabId, next)
  return true
}

export type SystemBackDecision =
  | { kind: 'closeOverlay' }
  | { kind: 'back'; tabId: string }
  | { kind: 'openWindowOverview' }
  | { kind: 'none' }

/** What the platform back hook does (`wm.system.back`, FR-019, contracts/wm-actions.md §5):
 * close an open window manager overlay; else go back in the active tab of the top visible window of the
 * active workspace; else, in compact mode, open the window overview. It never closes a window or
 * leaves the app. */
export function resolveSystemBack(
  state: WmState,
  histories: TabHistories,
  overlayOpen: boolean,
): SystemBackDecision {
  if (overlayOpen) return { kind: 'closeOverlay' }
  const top = frontmostWindow(state, state.activeWorkspaceId, true)
  const history = top ? histories.get(top.activeTabId) : undefined
  if (top && history && canGoBack(history))
    return { kind: 'back', tabId: top.activeTabId }
  return state.compact ? { kind: 'openWindowOverview' } : { kind: 'none' }
}

/** Drops the current entry of a tab because its target no longer exists (FR-027, research R10)
 * and moves to the neighbor in travel direction (`-1` back, `1` forward). `false` when the entry
 * is the only one, so the caller falls back to the start location. */
export function skipTabEntry(
  histories: TabHistories,
  tabId: string,
  direction: -1 | 1,
): boolean {
  const history = histories.get(tabId)
  if (!history) return false
  const next = removeEntry(history, history.index, direction)
  if (next === history) return false
  histories.set(tabId, next)
  return true
}
