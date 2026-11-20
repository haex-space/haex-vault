// Saving and restoring the window manager session (spec 022-session-restore, research R7,
// contracts/wm-session.md §4). Pure: the store (`stores/windowManager.ts`) passes its reactive
// state, the history map and the save queue (`composables/useWmSession.ts`), so this runs under
// `node scripts/check-wm-session.ts` without Vue or Pinia.
import type { AppDefinition } from './apps.ts'
import { hydrate } from './layoutState.ts'
import type { TabHistory } from './navigation.ts'
import {
  parseWmSession,
  snapshotSession,
  splitSession,
  type WmSession,
} from './session.ts'
import type { WmState } from './types.ts'

/** What `wm_session_restore_get`/`_set` return (mirrors `SessionRestoreState` in
 * `src/types/bindings/`): one value for the whole vault since spec 023 (FR-024). */
export type RestoreState = { enabled: boolean }

/** The part of `useWmSession()` this module drives. */
export type SessionPort = {
  saveNow(session: WmSession): void
  saveSoon(session: WmSession): void
  load(): Promise<{ restore: RestoreState; session: unknown }>
  setRestore(enabled: boolean): Promise<RestoreState>
  getRestore(): Promise<RestoreState>
  flushAsync(): Promise<void>
}

export function createSessionSync(deps: {
  state: WmState
  histories: Map<string, TabHistory>
  /** The apps a restored tab may run; read at restore time, so extensions loaded by then count. */
  apps: () => readonly AppDefinition[]
  port: SessionPort
  /** Re-syncs per-tab runtime and histories after `state` was replaced. */
  onRestored: () => void
}) {
  const { state, histories, port } = deps
  /** Whether the setting is on; `false` until restored (FR-003). */
  let enabled = false
  /** Whether `restoreAsync` has run. Until then the state is the initial empty workspace, which the
   * restore replaces. */
  let restored = false
  /** Apps opened before the restore, reopened in the restored state (they would be lost). */
  const openedEarly: Array<() => void> = []
  /** The setting changed elsewhere while the restore was under way; read it again after. */
  let refreshAfterRestore = false

  function snapshot(): WmSession {
    return snapshotSession(state, (tabId) => histories.get(tabId))
  }

  /** Saves at once (structural changes); does nothing while the setting does not apply. */
  function saveNow(): void {
    if (enabled) port.saveNow(snapshot())
  }

  /** Saves debounced (geometry, focus, navigation); does nothing while the setting does not
   * apply. */
  function saveSoon(): void {
    if (enabled) port.saveSoon(snapshot())
  }

  function replaceState(session: WmSession | null): void {
    const { layout, histories: saved } = session
      ? splitSession(session)
      : {
          layout: { workspaces: [], windows: [], activeWorkspaceId: '' },
          histories: new Map<string, TabHistory>(),
        }
    Object.assign(
      state,
      hydrate(layout, deps.apps(), state.area, state.compact),
    )
    histories.clear()
    for (const [tabId, history] of saved) histories.set(tabId, history)
    deps.onRestored()
  }

  /** Starts the vault session: restores the saved session if the setting applies and one is
   * stored, otherwise one empty workspace. Never throws (FR-012): a failed load or an invalid
   * session is logged and haex-vault starts empty. Then it reopens the apps opened meanwhile. */
  async function restoreAsync(): Promise<void> {
    try {
      await loadAndReplace()
    } finally {
      restored = true
      for (const reopen of openedEarly.splice(0)) {
        try {
          reopen()
        } catch (error) {
          console.error(
            '[wm] reopening an app opened before the restore failed',
            error,
          )
        }
      }
      if (refreshAfterRestore) {
        refreshAfterRestore = false
        await refreshRestoreAsync()
      }
    }
  }

  async function loadAndReplace(): Promise<void> {
    let loaded: { restore: RestoreState; session: unknown }
    try {
      loaded = await port.load()
    } catch (error) {
      console.error('[wm] wm_session_load failed; starting empty', error)
      enabled = false
      replaceState(null)
      return
    }
    enabled = loaded.restore.enabled
    const session =
      loaded.session == null ? null : parseWmSession(loaded.session)
    if (loaded.session != null && session === null)
      console.error('[wm] the saved session is not valid; starting empty')
    replaceState(enabled ? session : null)
  }

  /** Notes an app opened before the restore: `reopen` opens it again once the restored state is in
   * place. Does nothing afterwards. */
  function noteOpened(reopen: () => void): void {
    if (!restored) openedEarly.push(reopen)
  }

  /** Takes over the setting as stored now, after another device changed it through sync (spec 023
   * FR-024: one value for the vault). Before the restore it waits for it: the restore reads the
   * setting itself, and saving the initial empty state would overwrite the saved session. Never
   * throws: a failed read keeps the current setting. */
  async function refreshRestoreAsync(): Promise<void> {
    if (!restored) {
      refreshAfterRestore = true
      return
    }
    try {
      const restore = await port.getRestore()
      const wasEnabled = enabled
      applyRestore(restore)
      if (wasEnabled && !restore.enabled) {
        // wm_session_load removes this device's stale row when restore is off (FR-007/008).
        // Read the result again in case the setting changed while the cleanup was queued.
        try {
          applyRestore((await port.load()).restore)
        } catch (error) {
          console.error('[wm] removing the disabled session failed', error)
        }
      }
    } catch (error) {
      console.error('[wm] reading the session restore setting failed', error)
    }
  }

  /** Takes over a new setting (after `wm_session_restore_set`). Turning it on saves the current
   * session right away (FR-005); turning it off stops saving, the backend already deleted the
   * saved session (FR-007). */
  function applyRestore(restore: RestoreState): void {
    const wasEnabled = enabled
    enabled = restore.enabled
    if (enabled && !wasEnabled) saveNow()
  }

  /** Turns the setting on or off through the same queue as the saves, so no earlier save can
   * land after it, and takes over the result. */
  async function setRestoreAsync(enabled: boolean): Promise<RestoreState> {
    const restore = await port.setRestore(enabled)
    applyRestore(restore)
    return restore
  }

  return {
    saveNow,
    saveSoon,
    restoreAsync,
    noteOpened,
    refreshRestoreAsync,
    isRestored: () => restored,
    applyRestore,
    setRestoreAsync,
    getRestoreAsync: () => port.getRestore(),
    flushAsync: () => port.flushAsync(),
    isEnabled: () => enabled,
  }
}
