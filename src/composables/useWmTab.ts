import { inject, provide, type InjectionKey } from 'vue'
import type { useWindowManagerStore } from '~/stores/windowManager'
import { requestConfirmation } from '~/composables/useWmCloseConfirm'
import type { ActionHandler } from '~/lib/actions/handlers'
import type { TabLocation } from '~/lib/wm/navigation'
import type { CloseGuard } from '~/lib/wm/types'
import type { WmHeaderDescription } from '~/types/bindings/WmHeaderDescription'
import type { WmHeaderEvent } from '~/types/bindings/WmHeaderEvent'

/**
 * The window manager↔App contract (spec 015-workspace-shell, T019,
 * contracts/shell-app-contract.md). Apps import nothing from
 * `components/wm/` or `stores/windowManager.ts` — they only ever see this.
 */
export type WmTabApi = {
  readonly tabId: string
  readonly windowId: string
  /** The app this tab runs (spec 020: selects its route table). */
  readonly appId: string
  /** Marks the tab as "waiting on the user" (badge on the tab, Chevron entry, window overview,
   * Launcher, workspace switcher). */
  requestAttention(): void
  clearAttention(): void
  /** Overrides the tab's title dynamically; `null` reverts to the app definition's title. Never
   * persisted. */
  setTitle(title: string | null): void
  /** Registers a guard; a new one replaces the previous. Returns an unregister function. */
  registerCloseGuard(guard: CloseGuard): () => void
  /** Closes this tab (the window too, if it is the last tab) without asking any guard. */
  closeSelf(): void
  /** Opens another app, optionally at a location (spec 020 FR-012): a singleton already open is
   * activated and navigated there. */
  openApp(appId: string, at?: string | TabLocation): void
  /** Registers this tab's handler for one of its app's tab-bound actions (spec 020 research R19);
   * returns the unregister function. */
  registerActionHandler(actionId: string, handler: ActionHandler): () => void
  /** Hands holzi what to draw in this tab's window header (spec 090); replaces the previous
   * description whole, `null` clears it. Apps use `useWmHeader`, which calls this and clears it
   * when the app goes away. Never persisted. */
  setHeader(header: WmHeaderDescription | null): void
  /** Registers the handler of the events of this tab's header controls; a new one replaces the
   * previous. Returns an unregister function. */
  registerHeaderHandler(handler: (event: WmHeaderEvent) => void): () => void
}

const WM_TAB_KEY: InjectionKey<WmTabApi> = Symbol('wmTab')

/** Inert default so an app mounts outside a window manager too (e.g. this project's Node test harnesses,
 * which never provide a real window manager instance). */
const INERT_WM_TAB: WmTabApi = {
  tabId: '',
  windowId: '',
  appId: '',
  requestAttention() {},
  clearAttention() {},
  setTitle() {},
  registerCloseGuard() {
    return () => {}
  },
  closeSelf() {},
  openApp() {},
  registerActionHandler() {
    return () => {}
  },
  setHeader() {},
  registerHeaderHandler() {
    return () => {}
  },
}

/** Called by the window manager (one instance per rendered tab) to make the contract available to
 * whichever app mounts inside it. */
export function provideWmTab(api: WmTabApi): void {
  provide(WM_TAB_KEY, api)
}

/** Called by an app. Outside a window manager instance, returns `INERT_WM_TAB` rather than throwing. */
export function useWmTab(): WmTabApi {
  return inject(WM_TAB_KEY, INERT_WM_TAB)
}

/**
 * Shared "ask, then close" step behind every user-initiated window close (title bar, window
 * overview) — never `closeSelf()`, which the contract defines as skipping guards on purpose
 * (spec 015-workspace-shell, T031/T038, FR-014). `requestConfirmation` shows one
 * `wm/CloseConfirm.vue` dialog naming every guard's reason; declining leaves the window open.
 */
export async function requestCloseWindow(
  wm: ReturnType<typeof useWindowManagerStore>,
  windowId: string,
): Promise<boolean> {
  const results = wm.guardResultsFor(windowId)
  if (results.length > 0) {
    if (!(await requestConfirmation(results))) return false
    await Promise.all(results.map((result) => result.confirmAsync()))
  }
  wm.closeWindow(windowId)
  return true
}

/** The same "ask, then close" step for a single tab (T038) — used by the `wm.tab.close` action. */
export async function requestCloseTab(
  wm: ReturnType<typeof useWindowManagerStore>,
  windowId: string,
  tabId: string,
): Promise<boolean> {
  const result = wm.guardResultForTab(tabId)
  if (result) {
    if (!(await requestConfirmation([result]))) return false
    await result.confirmAsync()
  }
  wm.closeTab(windowId, tabId)
  return true
}

/** The same "ask, then delete" step for a workspace (015 FR-021, moved here from
 * `wm/WorkspaceOverview.vue` for spec 020's `wm.workspace.delete` action): deleting one with
 * open windows always confirms, naming the window count next to any guard reasons. Resolves
 * `false` when the user declines. */
export async function requestDeleteWorkspace(
  wm: ReturnType<typeof useWindowManagerStore>,
  workspaceId: string,
): Promise<boolean> {
  const windowCount = wm.windows.filter(
    (w) => w.workspaceId === workspaceId,
  ).length
  const guardResults = wm.guardResultsForWorkspace(workspaceId)
  const reasons = [
    ...(windowCount > 0
      ? [
          {
            reasonKey: 'wm.workspaces.deleteWindows',
            params: { count: windowCount },
          },
        ]
      : []),
    ...guardResults,
  ]
  if (reasons.length > 0) {
    if (!(await requestConfirmation(reasons))) return false
    await Promise.all(guardResults.map((result) => result.confirmAsync()))
  }
  wm.deleteWorkspace(workspaceId)
  return true
}
