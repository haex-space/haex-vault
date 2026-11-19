import { invoke } from '@tauri-apps/api/core'
import type { InstanceInfo } from '@bindings/InstanceInfo'
import type { CreateInstanceArgs } from '@bindings/CreateInstanceArgs'
import type { CreateInstanceResult } from '@bindings/CreateInstanceResult'
import type { OpenInstanceArgs } from '@bindings/OpenInstanceArgs'
import type { ImportInstanceArgs } from '@bindings/ImportInstanceArgs'
import type { ChangePassphraseArgs } from '@bindings/ChangePassphraseArgs'

/**
 * Thin wrapper around the Tauri command surface. Each function is a
 * one-shot `invoke()` that re-throws typed HaexError on failure.
 */
export function useInstance() {
  async function activeNameAsync(): Promise<string | null> {
    return await invoke<string | null>('active_instance_name')
  }

  async function listAsync(): Promise<InstanceInfo[]> {
    return await invoke<InstanceInfo[]>('list_instances')
  }

  async function createAsync(
    args: CreateInstanceArgs,
  ): Promise<CreateInstanceResult> {
    return await invoke<CreateInstanceResult>('create_instance', { args })
  }

  async function openAsync(args: OpenInstanceArgs): Promise<InstanceInfo> {
    return await invoke<InstanceInfo>('open_instance', { args })
  }

  /** Spec 043: copies a chosen vault file into the app's storage and opens the copy. */
  async function importAsync(
    args: ImportInstanceArgs,
  ): Promise<CreateInstanceResult> {
    return await invoke<CreateInstanceResult>('import_instance', { args })
  }

  async function closeAsync(): Promise<void> {
    await invoke('close_instance')
  }

  /** Spec 042: re-keys the active vault on this device; not an action, so no agent can call it. */
  async function changePassphraseAsync(
    args: ChangePassphraseArgs,
  ): Promise<void> {
    await invoke('change_vault_passphrase', { args })
  }

  return {
    activeNameAsync,
    listAsync,
    createAsync,
    openAsync,
    importAsync,
    closeAsync,
    changePassphraseAsync,
  }
}
