/**
 * Sync rules from the window (spec 061, contracts/tauri-commands.md): list, create, change, pause,
 * delete, run now, stop, and the deletions waiting for the user. A refused rule comes back as a `SyncRuleError` whose code the window
 * translates (`files.syncRules.error.<code>`).
 */
import { invoke } from '@tauri-apps/api/core'
import type { SyncRuleDraft } from '@bindings/SyncRuleDraft'
import type { SyncRuleError } from '@bindings/SyncRuleError'
import type { SyncRuleFileError } from '@bindings/SyncRuleFileError'
import type { SyncRulePending } from '@bindings/SyncRulePending'
import type { SyncRulePendingChoice } from '@bindings/SyncRulePendingChoice'
import type { SyncRuleView } from '@bindings/SyncRuleView'

/** `cause` as a rule error, if it is one. */
export function asSyncRuleError(cause: unknown): SyncRuleError | null {
  if (
    cause &&
    typeof cause === 'object' &&
    'code' in cause &&
    'message' in cause &&
    typeof (cause as SyncRuleError).code === 'string'
  ) {
    return cause as SyncRuleError
  }
  return null
}

export function useSyncRules() {
  async function listAsync(): Promise<SyncRuleView[]> {
    return await invoke<SyncRuleView[]>('sync_rules_list')
  }

  async function createAsync(draft: SyncRuleDraft): Promise<SyncRuleView> {
    return await invoke<SyncRuleView>('sync_rules_create', { draft })
  }

  async function updateAsync(
    id: string,
    draft: SyncRuleDraft,
  ): Promise<SyncRuleView> {
    return await invoke<SyncRuleView>('sync_rules_update', { id, draft })
  }

  async function setEnabledAsync(
    id: string,
    enabled: boolean,
  ): Promise<SyncRuleView> {
    return await invoke<SyncRuleView>('sync_rules_set_enabled', {
      id,
      enabled,
    })
  }

  async function deleteAsync(id: string): Promise<void> {
    await invoke('sync_rules_delete', { id })
  }

  async function runNowAsync(id: string): Promise<void> {
    await invoke('sync_rules_run_now', { id })
  }

  async function cancelAsync(id: string): Promise<void> {
    await invoke('sync_rules_cancel', { id })
  }

  async function pendingListAsync(id: string): Promise<SyncRulePending[]> {
    return await invoke<SyncRulePending[]>('sync_rules_pending_list', { id })
  }

  /** Applies `choice` at once; returns the files whose step failed, which stay in the list. */
  async function pendingResolveAsync(
    id: string,
    paths: string[],
    choice: SyncRulePendingChoice,
  ): Promise<SyncRuleFileError[]> {
    return await invoke<SyncRuleFileError[]>('sync_rules_pending_resolve', {
      id,
      paths,
      choice,
    })
  }

  return {
    listAsync,
    createAsync,
    updateAsync,
    setEnabledAsync,
    deleteAsync,
    runNowAsync,
    cancelAsync,
    pendingListAsync,
    pendingResolveAsync,
  }
}
