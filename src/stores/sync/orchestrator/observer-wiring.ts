/**
 * Sync Observer Wiring
 * Registers per-table reload callbacks so the right Pinia stores
 * refresh when CRDT sync brings in new rows.
 */

import { invoke } from '@tauri-apps/api/core'
import { registerStoreForTables } from '../syncEvents'
import { orchestratorLog as log } from './types'

/**
 * Wires every haex_* table that needs UI refresh after sync to the
 * matching store's loader. Called once during startSyncAsync.
 */
export const registerStoreReloadCallbacks = (): void => {
  registerStoreForTables(
    ['haex_extensions', 'haex_extension_migrations'],
    async () => {
      const extensionsStore = useExtensionsStore()
      await extensionsStore.loadExtensionsAsync()
    },
  )
  registerStoreForTables(
    ['haex_workspaces'],
    async () => {
      const workspaceStore = useWorkspaceStore()
      await workspaceStore.loadWorkspacesAsync()
    },
  )
  registerStoreForTables(
    ['haex_desktop_items'],
    async () => {
      const workspaceStore = useWorkspaceStore()
      if (!workspaceStore.currentWorkspace) {
        // Workspaces haven't loaded yet — the desktop store will refresh
        // itself once they do. Reloading desktop items now would early-return
        // and pollute logs during initial CRDT sync.
        return
      }
      const desktopStore = useDesktopStore()
      await desktopStore.loadDesktopItemsAsync()
    },
  )
  registerStoreForTables(
    ['haex_vault_settings'],
    async () => {
      const vaultSettingsStore = useVaultSettingsStore()
      await vaultSettingsStore.syncThemeAsync()
      await vaultSettingsStore.syncLocaleAsync()
      await vaultSettingsStore.syncVaultNameAsync()
    },
  )
  registerStoreForTables(
    ['haex_space_devices', 'haex_peer_shares'],
    async () => {
      const peerStore = usePeerStorageStore()
      await peerStore.loadSpaceDevicesAsync()
      await peerStore.loadSharesAsync()
      // Owner-Sync status text reads deviceStore.knownDevices to count the
      // owner's other devices; refresh it whenever space-devices change.
      const deviceStore = useDeviceStore()
      await deviceStore.loadKnownDevicesAsync()
      // Reload Rust-side allowed_peers: the daemon keeps its own in-memory
      // access control list and won't pick up new haex_space_devices rows
      // from CRDT until explicitly told to reload.
      try {
        await invoke('peer_storage_reload_shares')
      } catch (err) {
        log.warn(`peer_storage_reload_shares failed: ${err}`)
      }
    },
  )
  registerStoreForTables(
    ['haex_identities', 'haex_identity_claims'],
    async () => {
      const identityStore = useIdentityStore()
      await identityStore.loadIdentitiesAsync()

      // Update device claims for newly synced identities (e.g. second device)
      const deviceStore = useDeviceStore()
      if (deviceStore.deviceId) {
        await deviceStore.updateDeviceClaimsAsync()
      }
    },
  )
  registerStoreForTables(
    ['haex_spaces', 'haex_pending_invites'],
    async () => {
      const spacesStore = useSpacesStore()
      await spacesStore.loadSpacesFromDbAsync()
    },
  )
  registerStoreForTables(
    ['haex_space_members'],
    async () => {
      const spacesStore = useSpacesStore()
      await spacesStore.loadSpacesFromDbAsync()
      // After every membership-table sync, the leader of each local
      // space must rekey MLS for any disappeared member (forward
      // secrecy). Non-leaders skip internally.
      await spacesStore.reconcileMlsForLocalSpacesAsync()
    },
  )
  registerStoreForTables(
    ['haex_passwords_item_details', 'haex_passwords_item_tags', 'haex_passwords_tags'],
    async () => {
      const passwordsStore = usePasswordsStore()
      await passwordsStore.loadItemsAsync()
    },
  )
  registerStoreForTables(
    ['haex_passwords_groups', 'haex_passwords_group_items'],
    async () => {
      const groupsStore = usePasswordsGroupsStore()
      await groupsStore.loadGroupsAsync()
    },
  )
}
