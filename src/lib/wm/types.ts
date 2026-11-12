// Pure data types for the Workspace-Shell (spec 015-workspace-shell, T015).
// No Nuxt auto-imports: this module (and its siblings under src/lib/wm/)
// must load standalone under `node scripts/check-wm-state.ts`
// (plan research R6) — import other `./*.ts` siblings with an explicit
// extension, never a `~/` alias.
import type { WmHeaderDescription } from '../../types/bindings/WmHeaderDescription.ts'
import type { WmHeaderEvent } from '../../types/bindings/WmHeaderEvent.ts'

/** A geometric size, used for an app's default/minimum window size. */
export type Size = {
  width: number
  height: number
}

/** Below this window manager area width, the compact (full-area window) presentation applies (research
 * R7, spec.md Assumptions: a common tablet breakpoint). Shared by `layoutState.ts` (hydrate) and
 * `geometry.ts` (T027, live resize). */
export const COMPACT_MAX_WIDTH = 767

/** An ordered container for windows on one device (data-model.md `workspaces`). No stored name:
 * the UI derives "Arbeitsbereich N" from `position` (spec FR-019). */
export type Workspace = {
  id: string
  position: number
}

/** An opened app instance in exactly one window. `appId` is opaque (research R5/R9): unknown to
 * this feature's app registry is valid on the wire, so a future `extension.*` id needs no schema
 * change; `hydrate` drops a tab whose `appId` the registry does not resolve (FR-025). */
export type WmTab = {
  id: string
  appId: string
}

/** A frame with a title bar in exactly one workspace, holding one or more tabs. `x`/`y`/`width`/
 * `height` are the normal (non-maximized) geometry — maximizing never overwrites them (research
 * R7). `stack` is this device's window stacking rank (higher = further front). */
export type WmWindow = {
  id: string
  workspaceId: string
  x: number
  y: number
  width: number
  height: number
  minimized: boolean
  maximized: boolean
  stack: number
  tabs: WmTab[]
  activeTabId: string
}

/** The window manager's persisted-shape in-memory state for one device (data-model.md "Frontend-Zustand"). */
export type WmState = {
  workspaces: Workspace[]
  windows: WmWindow[]
  activeWorkspaceId: string
  /** The focused window in the active workspace; `null` when no window is open there. */
  activeWindowId: string | null
  /** Monotonically increasing; the source of the next `stack` value on focus/open. */
  nextStack: number
  /** The window manager's visible drawing area (the resizable region windows are clamped into). */
  area: Size
  /** Derived from the app window's width (`<= COMPACT_MAX_WIDTH`, research R7), not from `area`, which
   * the dock can narrow (spec 045 research R1); never written directly. */
  compact: boolean
}

/** What a `registerCloseGuard` callback (contracts/shell-app-contract.md) returns when the window manager
 * should ask the user before closing the tab; `null` means it may close without asking. */
export type CloseGuardResult = {
  /** i18n key of the reason shown in the confirmation (e.g. `wm.close.activeReply`). */
  reasonKey: string
  /** Runs after the user confirms; resolves once the tab may close safely. */
  confirmAsync: () => Promise<void>
}

export type CloseGuard = () => CloseGuardResult | null

/** Never persisted (data-model.md): per-tab runtime bookkeeping the store keeps alongside the
 * persisted `WmTab`, keyed by tab id. */
export type TabRuntime = {
  attention: boolean
  titleOverride: string | null
  guard: CloseGuard | null
  mounted: boolean
  /** What the tab's app handed holzi for its window's header (spec 090, data-model.md); kept while
   * the tab is in the background, gone with the tab. */
  header: WmHeaderDescription | null
  /** Gets the events of that header's controls. */
  headerHandler: ((event: WmHeaderEvent) => void) | null
}

/** The runtime of a tab that has just opened (and the inert one for a tab that has closed). */
export function newTabRuntime(): TabRuntime {
  return {
    attention: false,
    titleOverride: null,
    guard: null,
    mounted: false,
    header: null,
    headerHandler: null,
  }
}

/** What `hydrate` builds a state from: workspaces, windows and their plain tabs. A saved session
 * (`WmSession` in `session.ts`, spec 022) adds a version and each tab's history on top;
 * `splitSession` turns it into this. The empty start passes no workspaces and no windows. */
export type PersistedLayout = {
  workspaces: Workspace[]
  windows: WmWindow[]
  activeWorkspaceId: string
}
