import { computed, reactive, ref, toRefs, watch } from 'vue'
import { defineStore } from 'pinia'
import {
  getAppDefinition,
  tabTitleFor,
  type AppDefinition,
} from '~/lib/wm/apps'
import {
  allApps,
  extensionAppId,
  tabOfExtension,
  tabsOfStoppedExtensions,
} from '~/lib/extensions/apps'
import { useExtensionsStore } from '~/stores/extensions'
import {
  closeWindow as closeWindowReducer,
  createWorkspace as createWorkspaceReducer,
  deleteWorkspace as deleteWorkspaceReducer,
  focusWindow as focusWindowReducer,
  hydrate,
  minimizeWindow as minimizeWindowReducer,
  moveWindowToWorkspace as moveWindowToWorkspaceReducer,
  switchWorkspace as switchWorkspaceReducer,
  toggleMaximizeWindow as toggleMaximizeWindowReducer,
  updateArea as updateAreaReducer,
  updateWindowGeometry as updateWindowGeometryReducer,
} from '~/lib/wm/layoutState'
import {
  closeTab as closeTabReducer,
  switchTab as switchTabReducer,
} from '~/lib/wm/tabs'
import { currentLocation, type TabLocation } from '~/lib/wm/navigation'
import { titleForLocation } from '~/components/wm/appRoutes'
import {
  addTabAt,
  openAppAt,
  openAppInBackgroundAt,
} from '~/lib/wm/tabNavigation'
import type {
  CloseGuard,
  WmState,
  WmTab,
  WmWindow,
  Size,
  TabRuntime,
  Workspace,
} from '~/lib/wm/types'
import { createSessionSync, type RestoreState } from '~/lib/wm/sessionSync'
import { useWmSession } from '~/composables/useWmSession'
import { createWmGuards } from '~/stores/wmGuards'
import { createWmNavigation } from '~/stores/wmNavigation'

/**
 * window manager state and its public actions (spec 015-workspace-shell, T018,
 * contracts/shell-app-contract.md §4). Wraps the pure reducers from
 * `lib/wm/layoutState.ts` around one `reactive` `WmState`, so they can
 * keep mutating their `state` parameter in place while Vue tracks it.
 *
 * User Story 1-4's actions are implemented here: `openApp`, `addTab`,
 * `switchTab`, `focusWindow`, `closeWindow`, `closeTab`, `minimizeWindow`,
 * `toggleMaximizeWindow`, `updateWindowGeometry`, `createWorkspace`,
 * `deleteWorkspace`, `switchWorkspace`, `moveWindowToWorkspace`.
 *
 * Saving (spec 022-session-restore, `lib/wm/sessionSync.ts`): `state` starts as one empty
 * workspace, so the store is usable synchronously at setup. `restoreSessionAsync` (called once by
 * `pages/workspace/[instance].vue`, before anything else touches the store) replaces it in place
 * with the saved session if the setting "Sitzung wiederherstellen" applies on this device, keeping
 * `state`'s identity for every `toRefs`/`computed` consumer. Every mutating action then asks the
 * session sync to save: immediately for structural changes, debounced for geometry, focus and
 * navigation. While the setting does not apply (the default), nothing is saved.
 */
export const useWindowManagerStore = defineStore('windowManager', () => {
  const extensions = useExtensionsStore()
  /** holzi's apps and the installed extensions (spec 017); read when needed, so a newly installed
   * extension is an app at once. */
  function apps(): readonly AppDefinition[] {
    return allApps(extensions.apps)
  }

  const initialArea =
    typeof window === 'undefined'
      ? { width: 1280, height: 800 }
      : { width: window.innerWidth, height: window.innerHeight }
  const state = reactive<WmState>(
    hydrate(
      { workspaces: [], windows: [], activeWorkspaceId: '' },
      apps(),
      initialArea,
    ),
  )

  /** Never saved (data-model.md); per-tab bookkeeping keyed by tab id, kept in sync with
   * `state.windows[*].tabs` after every action that can add or remove a tab. A `Map` rather than
   * a plain object, so pruning a stale entry needs no dynamic-key `delete`. */
  const tabRuntime = reactive(new Map<string, TabRuntime>())

  // Per-tab history and the action runner (spec 020-tab-navigation, stores/wmNavigation.ts).
  const navigation = createWmNavigation({
    state,
    titleOverrideOf: (tabId) => tabRuntime.get(tabId)?.titleOverride ?? null,
    clearTitleOverride: (tabId) => setTabTitle(tabId, null),
    openApp: (appId) => openApp(appId),
  })

  const session = createSessionSync({
    state,
    histories: navigation.histories,
    apps,
    port: useWmSession(),
    onRestored: syncTabRuntime,
  })

  /** Whether `restoreSessionAsync` has run, and the apps opened before it are open again in the
   * restored session; tests wait for this before they open one. */
  const sessionRestored = ref(false)

  async function restoreSessionAsync(): Promise<void> {
    try {
      await session.restoreAsync()
    } finally {
      sessionRestored.value = true
    }
  }

  /** Retains runtime state for live tabs and initializes newly opened tabs. */
  function syncTabRuntime() {
    const liveIds = new Set(
      state.windows.flatMap((w) => w.tabs.map((t) => t.id)),
    )
    for (const id of tabRuntime.keys()) {
      if (!liveIds.has(id)) tabRuntime.delete(id)
    }
    for (const id of liveIds) {
      if (!tabRuntime.has(id)) {
        tabRuntime.set(id, {
          attention: false,
          titleOverride: null,
          guard: null,
          mounted: false,
        })
      }
    }
    navigation.syncNavigation()
  }
  syncTabRuntime()

  const windowsInActiveWorkspace = computed(() =>
    state.windows.filter((w) => w.workspaceId === state.activeWorkspaceId),
  )

  /** Returns inert defaults if the tab has already closed. */
  function runtimeFor(tabId: string): TabRuntime {
    return (
      tabRuntime.get(tabId) ?? {
        attention: false,
        titleOverride: null,
        guard: null,
        mounted: false,
      }
    )
  }

  /** Marks a tab's content mounted for this session (research R8: it then stays mounted, `v-show`
   * hides it instead of unmounting). Called by the window manager window the first time a tab becomes
   * visible (T031). */
  function markTabMounted(tabId: string) {
    const runtime = tabRuntime.get(tabId)
    if (runtime) runtime.mounted = true
  }

  /** The three `useWmTab()` setters (contracts/shell-app-contract.md) — a no-op for a tab id
   * `syncTabRuntime` has already pruned (the tab closed while an app's own async work was still
   * settling). */
  function setTabAttention(tabId: string, attention: boolean) {
    const runtime = tabRuntime.get(tabId)
    if (runtime) runtime.attention = attention
  }

  /** Overrides a live tab's displayed title without changing its app definition. */
  function setTabTitle(tabId: string, title: string | null) {
    const runtime = tabRuntime.get(tabId)
    if (runtime) runtime.titleOverride = title
  }

  /** Registers or clears the close guard for a live tab. */
  function setTabCloseGuard(tabId: string, guard: CloseGuard | null) {
    const runtime = tabRuntime.get(tabId)
    if (runtime) runtime.guard = guard
  }

  /** The active tab of a window, falling back to its first tab if `activeTabId` is somehow stale
   * (should not happen post-hydrate, but callers should not assume it never will). */
  function activeTabOf(window: WmWindow) {
    return (
      window.tabs.find((t) => t.id === window.activeTabId) ?? window.tabs[0]
    )
  }

  /** Raw display data for one tab — its title key (spec 020 research R9: the current location's
   * routed title with its params, else the app's), icon, any title override, and its attention
   * flag. Callers translate `titleKey` with `titleParams`; this store does not use `useI18n()`.
   * Shared by the tab bar, tab list, window title and overviews. */
  function tabDisplayInfo(tab: WmTab): {
    titleKey: string | undefined
    titleParams: Record<string, string>
    icon: string | undefined
    /** The app's own image (an extension's icon, a system app's coloured icon), shown instead of
     * `icon`. */
    iconUrl: string | undefined
    titleOverride: string | null
    hasAttention: boolean
    /** A development version of an extension (spec 017, US12). */
    dev: boolean
  } {
    const app = getAppDefinition(tab.appId, apps())
    const runtime = runtimeFor(tab.id)
    const history = navigation.historyOf(tab.id)
    const title = history
      ? tabTitleFor(
          app,
          titleForLocation(tab.appId, currentLocation(history).path),
        )
      : { key: app?.titleKey, params: {} }
    return {
      titleKey: title.key,
      titleParams: title.params,
      icon: app?.icon,
      iconUrl: app?.iconUrl,
      // An extension's own name is not translated; it stands where the app sets no title.
      titleOverride: runtime.titleOverride ?? app?.title ?? null,
      hasAttention: runtime.attention,
      dev: app?.dev === true,
    }
  }

  /** Raw display data for a window — its active tab's `tabDisplayInfo`, its tab count, and
   * whether *any* of its tabs wants attention (data-model.md's derived "window has attention"
   * rule — a window can have attention from a background tab even while its active tab does not).
   * Shared by `wm/Window.vue` and `wm/WindowOverview.vue` (T030). */
  function windowDisplayInfo(window: WmWindow): {
    titleKey: string | undefined
    titleParams: Record<string, string>
    icon: string | undefined
    iconUrl: string | undefined
    titleOverride: string | null
    tabCount: number
    hasAttention: boolean
  } | null {
    const tab = activeTabOf(window)
    if (!tab) return null
    return {
      ...tabDisplayInfo(tab),
      tabCount: window.tabs.length,
      hasAttention: window.tabs.some((t) => runtimeFor(t.id).attention),
    }
  }

  /** Opens the app, or activates its existing singleton tab, and saves at once (a deliberate click);
   * `background`: for a call of the agent, focus unchanged (spec 018, research R9). */
  function openApp(
    appId: string,
    at: string | TabLocation | null = null,
    background = false,
  ) {
    // The restore replaces the state: open it again there (a launcher click in the first moment).
    if (!background) session.noteOpened(() => openApp(appId, at))
    const title = (id: string) => runtimeFor(id).titleOverride
    const { tabId } = background
      ? openAppInBackgroundAt(state, navigation.histories, appId, apps())
      : openAppAt(state, navigation.histories, appId, at, apps(), title)
    syncTabRuntime()
    session.saveNow()
    return tabId
  }

  /** Adds a tab (at `at`, spec 020 FR-012) and saves at once. */
  function addTab(
    windowId: string,
    appId: string,
    at: string | TabLocation | null = null,
  ) {
    const { tabId } = addTabAt(
      state,
      navigation.histories,
      windowId,
      appId,
      at,
      apps(),
      (id) => runtimeFor(id).titleOverride,
    )
    syncTabRuntime()
    session.saveNow()
    return tabId
  }

  /** Activates a tab and saves its window's new active tab id. */
  function switchTab(windowId: string, tabId: string) {
    switchTabReducer(state, windowId, tabId)
    session.saveNow()
  }

  /** Removes the tab without asking anything — guard confirmation (FR-014) runs at the caller,
   * same as `closeWindow`. Closing the last tab closes the window too. */
  function closeTab(windowId: string, tabId: string) {
    closeTabReducer(state, windowId, tabId)
    syncTabRuntime()
    session.saveNow()
  }

  // Spec 017, FR-007/FR-039: the tabs of an extension disabled or removed on any own device close
  // here too, without asking (its frame can no longer answer anyway).
  watch(
    () => extensions.list,
    (list) => {
      if (!extensions.loaded) return
      for (const { windowId, tabId } of tabsOfStoppedExtensions(
        state.windows,
        list,
      ))
        closeTab(windowId, tabId)
    },
  )

  /** Brings an extension's tab forward, or opens the extension when it has none (a click on one
   * of its notifications, spec 017 US8). */
  function showExtension(extensionId: string): boolean {
    const found = tabOfExtension(state.windows, extensionId)
    if (!found) return openApp(extensionAppId(extensionId)) !== null
    focusWindow(found.windowId)
    switchTab(found.windowId, found.tabId)
    return true
  }

  /** Restores and raises a window, debouncing its new stack position. */
  function focusWindow(windowId: string) {
    focusWindowReducer(state, windowId)
    session.saveSoon()
  }

  /** Minimizes a window and saves the structural change immediately. */
  function minimizeWindow(windowId: string) {
    minimizeWindowReducer(state, windowId)
    session.saveNow()
  }

  /** Saves the window's maximized or restored state immediately. */
  function toggleMaximizeWindow(windowId: string) {
    toggleMaximizeWindowReducer(state, windowId)
    session.saveNow()
  }

  /** Applies a drag or resize result and debounces the resulting save. */
  function updateWindowGeometry(
    windowId: string,
    geometry: { x: number; y: number; width: number; height: number },
  ) {
    updateWindowGeometryReducer(state, windowId, geometry)
    session.saveSoon()
  }

  /** Keeps `state.area` in sync with the area windows get and `state.compact` with the app window's
   * width (T049; spec 045 research R1, `wm/Desktop.vue`'s size watcher) and re-clamps every
   * window's stored geometry into it — debounced like `updateWindowGeometry`, since a live resize
   * can fire rapidly too. */
  function updateArea(area: Size, viewportWidth: number) {
    if (updateAreaReducer(state, area, apps(), viewportWidth).length > 0)
      session.saveSoon()
  }

  /** Removes the window without asking anything — guard confirmation (FR-014) runs at the caller
   * (`useWmTab.ts`'s `requestCloseWindow`, T031; `wm/CloseConfirm.vue`, T038) before this is
   * invoked. */
  function closeWindow(windowId: string) {
    closeWindowReducer(state, windowId)
    syncTabRuntime()
    session.saveNow()
  }

  /** Creates a workspace with a fresh id and saves the session. */
  function createWorkspace(): Workspace {
    const workspace = createWorkspaceReducer(state, crypto.randomUUID())
    session.saveNow()
    return workspace
  }

  /** Activates a workspace and saves the session. */
  function switchWorkspace(workspaceId: string) {
    switchWorkspaceReducer(state, workspaceId)
    session.saveNow()
  }

  /** Deletes the workspace with its windows and tabs — guard confirmation for running replies
   * (FR-021) and the "close N windows" confirmation both run at the caller
   * (`wm/WorkspaceOverview.vue`, T040). A no-op for the last remaining workspace (I3). */
  function deleteWorkspace(workspaceId: string) {
    deleteWorkspaceReducer(state, workspaceId)
    syncTabRuntime()
    session.saveNow()
  }

  /** Moves a window with its tabs and immediately saves its new workspace id. */
  function moveWindowToWorkspace(windowId: string, workspaceId: string) {
    moveWindowToWorkspaceReducer(state, windowId, workspaceId)
    session.saveNow()
  }

  /** Whether *any* window in the workspace has attention (data-model.md's derived rule, mirroring
   * `windowDisplayInfo`'s own window-level derivation one layer up). */
  function workspaceHasAttention(workspaceId: string): boolean {
    return state.windows.some(
      (w) =>
        w.workspaceId === workspaceId && windowDisplayInfo(w)?.hasAttention,
    )
  }

  /** Whether any open tab of this app (any window, any workspace) wants attention (FR-015: the
   * Launcher is one of the surfaces a background window's attention must reach). */
  function appHasAttention(appId: string): boolean {
    return state.windows.some((w) =>
      w.tabs.some((tab) => tab.appId === appId && runtimeFor(tab.id).attention),
    )
  }

  // Close-guard queries for FR-014/FR-021 (stores/wmGuards.ts).
  const { guardResultsFor, guardResultForTab, guardResultsForWorkspace } =
    createWmGuards(state, (tabId) => tabRuntime.get(tabId)?.guard)

  /** Awaits the save queue before the vault locks or closes (FR-027) — `ChatApp.vue`'s `lock()`
   * already calls this before `useInstance().closeAsync()`. */
  function flushAsync(): Promise<void> {
    return session.flushAsync()
  }

  /** Turns the vault's "Sitzung wiederherstellen" on or off and takes over the result (spec
   * 022 FR-005, FR-007; one vault value since spec 023); the settings action calls this. */
  function setSessionRestore(enabled: boolean): Promise<RestoreState> {
    return session.setRestoreAsync(enabled)
  }

  // Navigation changes a tab's saved history (spec 022 clarification 2026-09-26).
  const navigate: typeof navigation.navigate = (...args) => {
    const changed = navigation.navigate(...args)
    session.saveSoon()
    return changed
  }
  const goTab: typeof navigation.go = (...args) => {
    const changed = navigation.go(...args)
    session.saveSoon()
    return changed
  }
  const skipCurrent: typeof navigation.skipCurrent = (...args) => {
    const changed = navigation.skipCurrent(...args)
    session.saveSoon()
    return changed
  }

  return {
    ...toRefs(state),
    apps,
    windowsInActiveWorkspace,
    restoreSessionAsync,
    sessionRestored,
    refreshSessionRestoreAsync: session.refreshRestoreAsync,
    setSessionRestore,
    getSessionRestore: session.getRestoreAsync,
    runtimeFor,
    tabDisplayInfo,
    windowDisplayInfo,
    markTabMounted,
    setTabAttention,
    setTabTitle,
    setTabCloseGuard,
    openApp,
    addTab,
    switchTab,
    focusWindow,
    showExtension,
    minimizeWindow,
    toggleMaximizeWindow,
    updateWindowGeometry,
    updateArea,
    closeWindow,
    closeTab,
    createWorkspace,
    switchWorkspace,
    deleteWorkspace,
    moveWindowToWorkspace,
    workspaceHasAttention,
    appHasAttention,
    guardResultsFor,
    guardResultForTab,
    guardResultsForWorkspace,
    flushAsync,
    overlays: navigation.overlays,
    systemBack: navigation.systemBack,
    historyOf: navigation.historyOf,
    navigate,
    goTab,
    skipCurrent,
    runAction: navigation.runAction,
    registerGlobalActionHandler: navigation.registerGlobalActionHandler,
    registerTabActionHandler: navigation.registerTabActionHandler,
  }
})
