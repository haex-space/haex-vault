import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { VaultInfo } from '@bindings/VaultInfo'

export const useLastVaultStore = defineStore('lastVaultStore', () => {
  const lastVaults = ref<VaultInfo[]>([])

  const syncLastVaultsAsync = async () => {
    lastVaults.value =
      (await listVaultsAsync()).sort(
        (a, b) => +new Date(`${b.lastAccess}`) - +new Date(`${a.lastAccess}`),
      ) ?? []

    return lastVaults.value
  }

  const listVaultsAsync = async () => {
    lastVaults.value = await invoke<VaultInfo[]>('list_vaults')
    return lastVaults.value
  }

  const removeVaultAsync = async (vaultName: string) => {
    return await invoke('delete_vault', { vaultName })
  }

  const moveVaultToTrashAsync = async (vaultName: string) => {
    return await invoke('move_vault_to_trash', { vaultName })
  }

  // Backend (database::import_delete::emit_vault_list_changed) fires this
  // whenever a vault is imported, deleted, or moved to trash through any code
  // path — UI drawer, Android share-intent (`import_vault_from_content_uri`),
  // or a direct Tauri command. The picker page only calls
  // `syncLastVaultsAsync` in `onMounted`, so without this listener mutations
  // that don't go through the drawer (e.g. share-intent into an already-
  // running app) leave the list stale until next remount.
  listen('vault-list-changed', () => {
    void syncLastVaultsAsync()
  })

  return {
    syncLastVaultsAsync,
    lastVaults,
    removeVaultAsync,
    moveVaultToTrashAsync,
  }
})
