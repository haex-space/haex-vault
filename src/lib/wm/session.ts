// The saved window manager session (spec 022-session-restore, data-model.md
// "WmSession"): what `wm_session_save` writes and `wm_session_load` returns.
// Pure data and validation, no Nuxt auto-imports (loads under
// `node scripts/check-wm-state.ts`).
import {
  createHistory,
  currentLocation,
  MAX_HISTORY_ENTRIES,
  type HistoryEntry,
  type TabHistory,
} from './navigation.ts'
import type { PersistedLayout, Workspace, WmState } from './types.ts'

export const WM_SESSION_VERSION = 1

/** One tab of a saved session: its app and its back/forward history (spec 020). */
export type WmSessionTab = {
  id: string
  appId: string
  history: TabHistory
}

export type WmSessionWindow = {
  id: string
  workspaceId: string
  x: number
  y: number
  width: number
  height: number
  minimized: boolean
  maximized: boolean
  stack: number
  tabs: WmSessionTab[]
  activeTabId: string
}

export type WmSession = {
  version: typeof WM_SESSION_VERSION
  workspaces: Workspace[]
  windows: WmSessionWindow[]
  activeWorkspaceId: string
}

/** A restored session split into what `hydrate` takes and each tab's history. */
export type RestoredSession = {
  layout: PersistedLayout
  histories: Map<string, TabHistory>
}

/** Takes the current state and every tab's history as one snapshot. A tab without a history
 * entry (should not happen) is saved at its app's start location.
 *
 * ponytail: every save writes the whole session, not just the window that changed (research R2).
 * Sessions are a few KB and saves are debounced, so diffing would only add the relational model
 * that spec 015 needed for sync. Oversized sessions fall back to `withoutHistories`. */
export function snapshotSession(
  state: Pick<WmState, 'workspaces' | 'windows' | 'activeWorkspaceId'>,
  historyOf: (tabId: string) => TabHistory | undefined,
): WmSession {
  return {
    version: WM_SESSION_VERSION,
    workspaces: state.workspaces.map((w) => ({
      id: w.id,
      position: w.position,
    })),
    windows: state.windows.map((w) => ({
      id: w.id,
      workspaceId: w.workspaceId,
      x: w.x,
      y: w.y,
      width: w.width,
      height: w.height,
      minimized: w.minimized,
      maximized: w.maximized,
      stack: w.stack,
      activeTabId: w.activeTabId,
      tabs: w.tabs.map((tab) => ({
        id: tab.id,
        appId: tab.appId,
        // JSON round trip, not structuredClone: the store's histories are reactive proxies.
        history: JSON.parse(
          JSON.stringify(historyOf(tab.id) ?? createHistory()),
        ) as TabHistory,
      })),
    })),
    activeWorkspaceId: state.activeWorkspaceId,
  }
}

/** Reduces every tab's history to its current entry: the fallback when a session with full
 * histories exceeds the backend's size limit (data-model.md "Größe"). */
export function withoutHistories(session: WmSession): WmSession {
  return {
    ...session,
    windows: session.windows.map((w) => ({
      ...w,
      tabs: w.tabs.map((tab) => ({
        ...tab,
        history: createHistory(currentLocation(tab.history)),
      })),
    })),
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value)
}

function parseEntry(value: unknown): HistoryEntry | null {
  if (!isRecord(value) || !isRecord(value.location)) return null
  const { path, query } = value.location
  if (typeof path !== 'string' || !path.startsWith('/') || !isRecord(query))
    return null
  const pairs = Object.entries(query)
  if (!pairs.every(([, v]) => typeof v === 'string')) return null
  if (value.title !== null && typeof value.title !== 'string') return null
  return {
    location: {
      path,
      query: Object.fromEntries(pairs) as Record<string, string>,
    },
    title: value.title,
  }
}

/** A valid history, or `null` (the caller then starts the tab fresh). */
function parseHistory(value: unknown): TabHistory | null {
  if (!isRecord(value) || !Array.isArray(value.entries)) return null
  const { entries, index } = value
  if (entries.length < 1 || entries.length > MAX_HISTORY_ENTRIES) return null
  if (
    !Number.isInteger(index) ||
    (index as number) < 0 ||
    (index as number) >= entries.length
  )
    return null
  const parsed = entries.map(parseEntry)
  if (parsed.some((entry) => entry === null)) return null
  return { entries: parsed as HistoryEntry[], index: index as number }
}

function parseTab(value: unknown): WmSessionTab | null {
  if (!isRecord(value)) return null
  if (typeof value.id !== 'string' || typeof value.appId !== 'string')
    return null
  return {
    id: value.id,
    appId: value.appId,
    // A broken history does not cost the tab (data-model.md "Beim Laden").
    history: parseHistory(value.history) ?? createHistory(),
  }
}

function parseWindow(value: unknown): WmSessionWindow | null {
  if (!isRecord(value) || !Array.isArray(value.tabs)) return null
  const {
    id,
    workspaceId,
    x,
    y,
    width,
    height,
    minimized,
    maximized,
    stack,
    activeTabId,
  } = value
  if (typeof id !== 'string' || typeof workspaceId !== 'string') return null
  if (typeof activeTabId !== 'string') return null
  if (![x, y, width, height, stack].every(isFiniteNumber)) return null
  if (typeof minimized !== 'boolean' || typeof maximized !== 'boolean')
    return null
  const tabs = value.tabs.map(parseTab)
  if (tabs.some((tab) => tab === null)) return null
  return {
    id,
    workspaceId,
    x: x as number,
    y: y as number,
    width: width as number,
    height: height as number,
    minimized,
    maximized,
    stack: stack as number,
    activeTabId,
    tabs: tabs as WmSessionTab[],
  }
}

function parseWorkspace(value: unknown): Workspace | null {
  if (!isRecord(value) || typeof value.id !== 'string') return null
  if (!isFiniteNumber(value.position)) return null
  return { id: value.id, position: value.position }
}

/** Validates a loaded session. Anything malformed at the session, workspace or window level
 * yields `null` and haex-vault starts empty (FR-012); a single bad tab history is replaced by a fresh
 * one. Unknown apps, duplicate singletons and geometry are `hydrate`'s job. */
export function parseWmSession(value: unknown): WmSession | null {
  if (!isRecord(value) || value.version !== WM_SESSION_VERSION) return null
  if (!Array.isArray(value.workspaces) || !Array.isArray(value.windows))
    return null
  if (typeof value.activeWorkspaceId !== 'string') return null
  const workspaces = value.workspaces.map(parseWorkspace)
  const windows = value.windows.map(parseWindow)
  if (workspaces.some((w) => w === null) || windows.some((w) => w === null))
    return null
  return {
    version: WM_SESSION_VERSION,
    workspaces: workspaces as Workspace[],
    windows: windows as WmSessionWindow[],
    activeWorkspaceId: value.activeWorkspaceId,
  }
}

/** Splits a session into `hydrate`'s input (tabs without history) and the histories by tab id. */
export function splitSession(session: WmSession): RestoredSession {
  const histories = new Map<string, TabHistory>()
  const layout: PersistedLayout = {
    workspaces: session.workspaces,
    windows: session.windows.map((w) => ({
      ...w,
      tabs: w.tabs.map((tab) => {
        histories.set(tab.id, tab.history)
        return { id: tab.id, appId: tab.appId }
      }),
    })),
    activeWorkspaceId: session.activeWorkspaceId,
  }
  return { layout, histories }
}
