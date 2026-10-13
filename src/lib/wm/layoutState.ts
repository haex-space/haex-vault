// Window/workspace reducers and hydration for the Workspace-Shell (spec 015-workspace-shell,
// T016). Pure functions over `WmState` — no Nuxt auto-imports, relative sibling imports with
// an explicit .ts extension (plan research R6), so this loads standalone under
// `node scripts/check-wm-state.ts`.
import type { AppDefinition } from './apps.ts'
import { getAppDefinition } from './apps.ts'
import { cascadePosition, clampGeometry } from './geometry.ts'
import {
  COMPACT_MAX_WIDTH,
  type PersistedLayout,
  type WmState,
  type WmTab,
  type WmWindow,
  type Size,
  type Workspace,
} from './types.ts'

/** Every open tab of `appId` across every workspace (FR-016 singleton search is device-wide, not
 * just the active workspace). Exported for `tabs.ts`'s `addTab` (T033), the same singleton search
 * `openApp` uses here. */
export function findOpenTab(
  state: WmState,
  appId: string,
): { window: WmWindow; tab: WmTab } | null {
  for (const window of state.windows) {
    const tab = window.tabs.find((t) => t.appId === appId)
    if (tab) return { window, tab }
  }
  return null
}

/** The front-most window in `workspaceId` (optionally excluding minimized ones) — shared by
 * `closeWindow`/`minimizeWindow`'s "who becomes active next" step, and (T039) `deleteWorkspace`'s
 * and `moveWindowToWorkspace`'s. */
export function frontmostWindow(
  state: WmState,
  workspaceId: string,
  visibleOnly = false,
): WmWindow | null {
  return state.windows
    .filter(
      (w) => w.workspaceId === workspaceId && (!visibleOnly || !w.minimized),
    )
    .reduce<WmWindow | null>(
      (front, w) => (front === null || w.stack > front.stack ? w : front),
      null,
    )
}

/** Restores (if minimized), brings to front, and activates `tab` within its window and workspace
 * — the shared "make this tab visible" step behind `openApp`'s singleton path and, later, tab
 * selection (tabs.ts, T033). */
export function focusWindow(state: WmState, windowId: string): void {
  const window = state.windows.find((w) => w.id === windowId)
  if (!window) return
  window.minimized = false
  window.stack = ++state.nextStack
  state.activeWindowId = window.id
  state.activeWorkspaceId = window.workspaceId
}

/** Minimizes the window; if it was active, the next-front-most *visible* window in the same
 * workspace becomes active (data-model.md's Fenster transitions). */
export function minimizeWindow(state: WmState, windowId: string): void {
  const window = state.windows.find((w) => w.id === windowId)
  if (!window) return
  window.minimized = true
  if (state.activeWindowId !== windowId) return
  state.activeWindowId =
    frontmostWindow(state, window.workspaceId, true)?.id ?? null
}

/** Toggles maximized state (FR-039) and focuses the window — the stored normal geometry is never
 * touched either way (research R7); `geometry.ts`'s `windowDisplayRect` resolves what actually
 * renders. */
export function toggleMaximizeWindow(state: WmState, windowId: string): void {
  const window = state.windows.find((w) => w.id === windowId)
  if (!window) return
  window.maximized = !window.maximized
  focusWindow(state, windowId)
}

/** Applies a drag/resize result (already clamped by the caller, `useWindowPointerGesture`) to the
 * window's stored normal geometry. */
export function updateWindowGeometry(
  state: WmState,
  windowId: string,
  geometry: { x: number; y: number; width: number; height: number },
): void {
  const window = state.windows.find((w) => w.id === windowId)
  if (!window) return
  window.x = geometry.x
  window.y = geometry.y
  window.width = geometry.width
  window.height = geometry.height
}

/** For a singleton app already open somewhere, activates its existing tab (device-wide search,
 * FR-016) and reports `true` — `openApp`'s and `tabs.ts`'s `addTab`'s shared first step. `false`
 * means the caller should proceed to create a new tab/window. */
export function activateExistingSingleton(
  state: WmState,
  app: AppDefinition,
  appId: string,
): boolean {
  if (app.multiInstance) return false
  const existing = findOpenTab(state, appId)
  if (!existing) return false
  existing.window.activeTabId = existing.tab.id
  focusWindow(state, existing.window.id)
  return true
}

/** A new window of `app` with one tab, placed like every new window in the active workspace. */
function newAppWindow(
  state: WmState,
  app: AppDefinition,
  appId: string,
  minimized: boolean,
): WmWindow {
  const tabId = crypto.randomUUID()
  const openInWorkspace = state.windows.filter(
    (w) => w.workspaceId === state.activeWorkspaceId,
  ).length
  const { x, y } = cascadePosition(openInWorkspace, app.defaultSize, state.area)
  // A dock (or a compact window) can leave less room than an app's preferred size.  New windows
  // must fit the current window area just like restored and resized windows; otherwise their
  // content can end up underneath the dock and controls at the bottom become unclickable.
  const geometry = clampGeometry(
    { x, y, width: app.defaultSize.width, height: app.defaultSize.height },
    app.minSize,
    state.area,
  )
  const window: WmWindow = {
    id: crypto.randomUUID(),
    workspaceId: state.activeWorkspaceId,
    x: geometry.x,
    y: geometry.y,
    width: geometry.width,
    height: geometry.height,
    minimized,
    maximized: false,
    stack: ++state.nextStack,
    tabs: [{ id: tabId, appId }],
    activeTabId: tabId,
  }
  state.windows.push(window)
  return window
}

/** Opens `appId` as a new window, or activates its existing tab if it is a singleton app already
 * open somewhere (FR-016). No-op for an `appId` the registry does not resolve — callers (Launcher,
 * legacy-route redirect) are expected to only ever pass a known id. */
export function openApp(
  state: WmState,
  appId: string,
  apps: readonly AppDefinition[] = [],
): void {
  const app = getAppDefinition(appId, apps)
  if (!app) return
  if (activateExistingSingleton(state, app, appId)) return
  state.activeWindowId = newAppWindow(state, app, appId, false).id
}

/** The tab of `appId` that serves a call of the agent (spec 018, research R9), without touching the
 * user's window, workspace or focus: a tab already open in the active workspace (only those are
 * mounted), else a new minimized window there. The compact layout hides minimized windows the same
 * way, so it needs nothing else. `null` for an unknown app and for a singleton open in another
 * workspace, which cannot serve here and must not open twice. */
export function openAppInBackground(
  state: WmState,
  appId: string,
  apps: readonly AppDefinition[] = [],
): string | null {
  const app = getAppDefinition(appId, apps)
  if (!app) return null
  for (const window of state.windows) {
    if (window.workspaceId !== state.activeWorkspaceId) continue
    const tab = window.tabs.find((t) => t.appId === appId)
    if (tab) return tab.id
  }
  if (!app.multiInstance && findOpenTab(state, appId)) return null
  return newAppWindow(state, app, appId, true).activeTabId
}

/** Removes the window (and its tabs) without asking anything — guard confirmation (FR-014) runs
 * at the store/UI layer before this is called (data-model.md). If it was active, the next
 * front-most *visible* window of its workspace becomes active, as after `minimizeWindow`: a
 * minimized window must not receive the focused window's shortcuts. */
export function closeWindow(state: WmState, windowId: string): void {
  const closed = state.windows.find((w) => w.id === windowId)
  if (!closed) return
  const wasActive = state.activeWindowId === windowId
  state.windows = state.windows.filter((w) => w.id !== windowId)
  if (!wasActive) return
  state.activeWindowId =
    frontmostWindow(state, closed.workspaceId, true)?.id ?? null
}

/** Appends a new, empty workspace at the end (FR-019). Its id is provided by the caller —
 * workspace ids are backend-assigned (research R5); until Phase 7 wires `wm_create_workspace`,
 * the store generates a temporary local one. Does not switch to it (contracts/tauri-commands.md:
 * the caller follows up with `switchWorkspace`). */
export function createWorkspace(state: WmState, id: string): Workspace {
  const workspace: Workspace = { id, position: state.workspaces.length }
  state.workspaces.push(workspace)
  return workspace
}

/** Activates `workspaceId`, if it exists, and its front-most visible window: the active window
 * always lies in the active workspace (data-model.md), so shortcuts never reach a window of a
 * workspace that is not shown. */
export function switchWorkspace(state: WmState, workspaceId: string): void {
  if (!state.workspaces.some((w) => w.id === workspaceId)) return
  if (state.activeWorkspaceId === workspaceId) return
  state.activeWorkspaceId = workspaceId
  state.activeWindowId = frontmostWindow(state, workspaceId, true)?.id ?? null
}

/** Deletes a workspace and its windows/tabs (FR-021 — confirmation runs at the store/UI layer
 * first); a no-op for the last remaining workspace (I3). Positions are re-densified afterward
 * (I2). If the deleted workspace was active, its previous neighbor becomes active, or the next one
 * if it was first (data-model.md's Workspace transitions). */
export function deleteWorkspace(state: WmState, workspaceId: string): void {
  if (state.workspaces.length <= 1) return
  const index = state.workspaces.findIndex((w) => w.id === workspaceId)
  if (index === -1) return
  state.windows = state.windows.filter((w) => w.workspaceId !== workspaceId)
  state.workspaces.splice(index, 1)
  state.workspaces.forEach((w, i) => {
    w.position = i
  })
  if (state.activeWorkspaceId !== workspaceId) return
  const neighborIndex = Math.max(0, index - 1)
  const neighbor = state.workspaces.find((_, i) => i === neighborIndex)
  state.activeWorkspaceId = neighbor?.id ?? ''
  state.activeWindowId =
    frontmostWindow(state, state.activeWorkspaceId, true)?.id ?? null
}

/** Moves a window (with all its tabs) to another workspace without touching its content (FR-020).
 * If it was the active window and it just left the active workspace, the next front-most visible
 * window remaining there becomes active instead (same rule as `closeWindow`/`minimizeWindow`). */
export function moveWindowToWorkspace(
  state: WmState,
  windowId: string,
  workspaceId: string,
): void {
  const window = state.windows.find((w) => w.id === windowId)
  if (!window) return
  if (!state.workspaces.some((w) => w.id === workspaceId)) return
  if (window.workspaceId === workspaceId) return
  const wasActive = state.activeWindowId === windowId
  window.workspaceId = workspaceId
  if (!wasActive) return
  state.activeWindowId =
    frontmostWindow(state, state.activeWorkspaceId, true)?.id ?? null
}

/**
 * Builds a fresh `WmState` from a persisted layout (or an empty one, before Phase 7 wires the
 * real backend call). Steps match data-model.md's "Wiederherstellen": drop tabs with an
 * unresolvable `appId` and any window left without one (FR-025); repair a dangling
 * `activeTabId`/`workspaceId`; clamp geometry into `area` (FR-026); re-derive dense `stack` from
 * the persisted order; fall back to one default workspace if none survive (FR-018, FR-025).
 * `compact` follows the app window's width (spec 045 research R1), which `area` no longer equals
 * once the dock takes space; without it, `area`'s width stands in, as at first load.
 */
export function hydrate(
  layout: PersistedLayout,
  apps: readonly AppDefinition[],
  area: Size,
  compact: boolean = area.width <= COMPACT_MAX_WIDTH,
): WmState {
  const defaultWorkspace: Workspace =
    layout.workspaces.reduce<Workspace | null>(
      (min, w) => (min === null || w.position < min.position ? w : min),
      null,
    ) ?? { id: crypto.randomUUID(), position: 0 }
  const workspaces: Workspace[] =
    layout.workspaces.length > 0
      ? [...layout.workspaces].sort((a, b) => a.position - b.position)
      : [defaultWorkspace]
  const workspaceIds = new Set(workspaces.map((w) => w.id))
  const singletonAppIds = new Set(
    apps.filter((app) => !app.multiInstance).map((app) => app.id),
  )
  const hydratedSingletonAppIds = new Set<string>()

  const windows: WmWindow[] = []
  let stack = 0
  for (const source of [...layout.windows].sort((a, b) => a.stack - b.stack)) {
    const tabs = source.tabs.filter((tab) => {
      if (getAppDefinition(tab.appId, apps) === undefined) return false
      if (!singletonAppIds.has(tab.appId)) return true
      if (hydratedSingletonAppIds.has(tab.appId)) return false
      hydratedSingletonAppIds.add(tab.appId)
      return true
    })
    const firstTab = tabs.find(() => true)
    if (!firstTab) continue
    const activeTabId = tabs.some((tab) => tab.id === source.activeTabId)
      ? source.activeTabId
      : firstTab.id
    const workspaceId = workspaceIds.has(source.workspaceId)
      ? source.workspaceId
      : defaultWorkspace.id
    const firstApp = getAppDefinition(firstTab.appId, apps)
    const geometry = clampGeometry(
      source,
      firstApp?.minSize ?? { width: 0, height: 0 },
      area,
    )
    const window: WmWindow = {
      ...source,
      ...geometry,
      workspaceId,
      tabs,
      activeTabId,
      stack: stack++,
    }
    windows.push(window)
  }

  const activeWorkspaceId = workspaceIds.has(layout.activeWorkspaceId)
    ? layout.activeWorkspaceId
    : defaultWorkspace.id

  const result: WmState = {
    workspaces,
    windows,
    activeWorkspaceId,
    activeWindowId: null,
    nextStack: stack,
    area,
    compact,
  }
  result.activeWindowId =
    frontmostWindow(result, activeWorkspaceId, true)?.id ?? null
  return result
}

/** Keeps the window manager's live area in sync with the area its windows actually get (T049; spec 045:
 * the app window minus a space-reserving dock), recomputing `compact` from `viewportWidth`, the app
 * window's own width (spec 045 research R1), and re-clamping every window's stored (normal) geometry into the new area — the same clamp
 * `hydrate` applies at load time (FR-026), now also on a live resize, so shrinking never leaves a
 * window positioned or sized outside the visible area. Compact/maximized display itself needs no
 * clamp here: `windowDisplayRect` (geometry.ts) already resolves that per-render without touching
 * the stored geometry this function corrects (research R7, FR-028's "remembered geometry"
 * survives the compact/normal switch).
 *
 * Uses the widest minSize across a window's own tabs, matching `wm/Window.vue`'s own
 * interactive-resize clamp — not just its first tab's, the way `hydrate` above approximates it.
 *
 * Returns the ids of windows whose geometry actually changed, so the caller (the store) knows
 * which ones still need persisting. */
export function updateArea(
  state: WmState,
  area: Size,
  apps: readonly AppDefinition[],
  viewportWidth: number,
): string[] {
  state.area = area
  state.compact = viewportWidth <= COMPACT_MAX_WIDTH

  const changedWindowIds: string[] = []
  for (const window of state.windows) {
    let minWidth = 0
    let minHeight = 0
    for (const tab of window.tabs) {
      const app = getAppDefinition(tab.appId, apps)
      if (app) {
        minWidth = Math.max(minWidth, app.minSize.width)
        minHeight = Math.max(minHeight, app.minSize.height)
      }
    }
    const clamped = clampGeometry(
      window,
      { width: minWidth, height: minHeight },
      area,
    )
    if (
      clamped.x !== window.x ||
      clamped.y !== window.y ||
      clamped.width !== window.width ||
      clamped.height !== window.height
    ) {
      window.x = clamped.x
      window.y = clamped.y
      window.width = clamped.width
      window.height = clamped.height
      changedWindowIds.push(window.id)
    }
  }
  return changedWindowIds
}
