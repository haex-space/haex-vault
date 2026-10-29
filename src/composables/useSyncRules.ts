/**
 * Sync rules from the window (spec 061, contracts/tauri-commands.md): list, create, change, pause,
 * delete, run now, stop, the preview, the exclusion file, the deletions waiting for the user, and
 * the run log with its depth. A refused rule comes back as a `SyncRuleError` whose code the window
 * translates (`files.syncRules.error.<code>`).
 */
import { invoke } from '@tauri-apps/api/core'
import type { SyncRuleDraft } from '@bindings/SyncRuleDraft'
import type { SyncRuleError } from '@bindings/SyncRuleError'
import type { SyncRuleFileError } from '@bindings/SyncRuleFileError'
import type { SyncRuleIgnore } from '@bindings/SyncRuleIgnore'
import type { SyncRulePlan } from '@bindings/SyncRulePlan'
import type { SyncRuleRun } from '@bindings/SyncRuleRun'
import type { SyncRuleRunItem } from '@bindings/SyncRuleRunItem'
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

  /** What a run would do; `id` alone for a stored rule, `draft` for a new one, both for a change. */
  async function previewAsync(target: {
    id?: string
    draft?: SyncRuleDraft
  }): Promise<SyncRulePlan> {
    return await invoke<SyncRulePlan>('sync_rules_preview', {
      id: target.id ?? null,
      draft: target.draft ?? null,
    })
  }

  async function ignoreGetAsync(id: string): Promise<SyncRuleIgnore> {
    return await invoke<SyncRuleIgnore>('sync_rules_ignore_get', { id })
  }

  async function ignoreSetAsync(
    id: string,
    content: string,
  ): Promise<SyncRuleIgnore> {
    return await invoke<SyncRuleIgnore>('sync_rules_ignore_set', {
      id,
      content,
    })
  }

  async function runsAsync(id: string): Promise<SyncRuleRun[]> {
    return await invoke<SyncRuleRun[]>('sync_rules_runs', { id })
  }

  async function runItemsAsync(
    runId: string,
    offset: number,
    limit: number,
  ): Promise<SyncRuleRunItem[]> {
    return await invoke<SyncRuleRunItem[]>('sync_rules_run_items', {
      runId,
      offset,
      limit,
    })
  }

  async function logDepthGetAsync(): Promise<number> {
    return await invoke<number>('sync_rules_log_depth_get')
  }

  async function logDepthSetAsync(depth: number): Promise<void> {
    await invoke('sync_rules_log_depth_set', { depth })
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
    previewAsync,
    ignoreGetAsync,
    ignoreSetAsync,
    runsAsync,
    runItemsAsync,
    logDepthGetAsync,
    logDepthSetAsync,
  }
}
