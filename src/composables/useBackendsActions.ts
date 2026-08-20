import { DidAuthAction } from '@haex-space/ucan'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { decryptVaultNameAsync } from '@/utils/crypto/vaultName'
import type { SelectHaexSyncBackends } from '~/database/schemas'

export interface ServerVault {
  spaceId: string
  encryptedVaultName: string
  vaultNameNonce: string
  vaultNameSalt: string
  ephemeralPublicKey: string
  createdAt: string
  decryptedName?: string
}

export interface GroupedServerVaults {
  backend: SelectHaexSyncBackends
  vaults: ServerVault[]
  isLoading: boolean
  error: string | null
  currentVaultMissingOnServer: boolean
}

export const useBackendsActions = () => {
  const { t } = useI18n()
  const { add } = useToast()

  const syncBackendsStore = useSyncBackendsStore()
  const syncEngineStore = useSyncEngineStore()
  const syncOrchestratorStore = useSyncOrchestratorStore()
  const vaultStore = useVaultStore()

  const { backends: syncBackends } = storeToRefs(syncBackendsStore)
  const { currentVaultId } = storeToRefs(vaultStore)

  const groupedServerVaults = ref<GroupedServerVaults[]>([])
  const isReUploading = ref(false)

  const groupedVaultsMap = computed(() => {
    const map = new Map<string, GroupedServerVaults>()
    for (const g of groupedServerVaults.value) {
      map.set(g.backend.id, g)
    }
    return map
  })

  const getGroupedVaults = (backendId: string) => {
    return groupedVaultsMap.value.get(backendId)
  }

  // Load vaults for a specific backend
  const loadVaultsForBackendAsync = async (
    backend: SelectHaexSyncBackends,
  ): Promise<ServerVault[]> => {
    try {
      const identityStore = useIdentityStore()
      const resolved = await identityStore.getIdentityByIdAsync(backend.identityId)
      if (!resolved?.privateKey) {
        throw new Error('Identity not found or incomplete')
      }
      const identity = { privateKey: resolved.privateKey, did: resolved.did }

      // Fetch vaults from server using DID-Auth
      const response = await fetchWithDidAuth(
        `${backend.homeServerUrl}/sync/vaults`,
        identity.privateKey,
        identity.did,
        DidAuthAction.VaultList,
      )

      if (!response.ok) {
        throw new Error('Failed to fetch vaults')
      }

      const data = await response.json()
      const vaults: ServerVault[] = data.vaults

      // Decrypt vault names using identity Ed25519 private key (Rust: Ed25519→X25519 + ECDH + AES-GCM)
      await Promise.all(
        vaults.map(async (vault) => {
          try {
            vault.decryptedName = await decryptVaultNameAsync(
              vault.encryptedVaultName,
              vault.vaultNameNonce,
              vault.vaultNameSalt,
              vault.ephemeralPublicKey,
              identity.privateKey,
            )
          } catch (e) {
            console.warn('[SYNC] Failed to decrypt vault name:', e)
          }
        }),
      )

      return vaults
    } catch (error) {
      console.error(`Failed to load vaults for backend ${backend.name}:`, error)
      throw error
    }
  }

  // Auto-load all server vaults grouped by backend
  const loadAllServerVaultsAsync = async () => {
    if (syncBackends.value.length === 0) {
      return
    }

    // Initialize grouped vaults structure, preserving existing data for disabled backends
    const previousGroups = groupedServerVaults.value
    groupedServerVaults.value = syncBackends.value.map((backend) => {
      const existing = previousGroups.find((g) => g.backend.id === backend.id)
      if (!backend.enabled && existing && !existing.isLoading) {
        // Keep previously loaded vaults for disabled backends
        return { ...existing, backend }
      }
      return {
        backend,
        vaults: [],
        isLoading: backend.enabled,
        error: null,
        currentVaultMissingOnServer: false,
      }
    })

    // Load vaults for each enabled backend in parallel
    await Promise.allSettled(
      groupedServerVaults.value.map(async (group) => {
        if (!group.backend.enabled) {
          return
        }

        try {
          const vaults = await loadVaultsForBackendAsync(group.backend)

          // Keep all vaults including the currently opened one
          group.vaults = vaults
          group.isLoading = false

          // Check if this backend is configured for current vault but vault is not on server
          if (group.backend.spaceId === currentVaultId.value) {
            const vaultFoundOnServer = vaults.some(
              (v) => v.spaceId === currentVaultId.value,
            )
            group.currentVaultMissingOnServer = !vaultFoundOnServer
          }
        } catch (error) {
          group.error = error instanceof Error ? error.message : 'Unknown error'
          group.isLoading = false
        }
      }),
    )
  }

  // Toggle backend enabled/disabled
  const toggleBackendAsync = async (backendId: string) => {
    const backend = syncBackends.value.find((b) => b.id === backendId)
    if (!backend) return

    try {
      const newEnabledState = !backend.enabled

      await syncBackendsStore.updateBackendAsync(backendId, {
        enabled: newEnabledState,
      })

      // Start/stop sync based on new state
      if (newEnabledState) {
        // Initialize token manager for this backend
        syncEngineStore.initTokenManagerAsync(backendId)

        // Start sync
        await syncOrchestratorStore.startSyncAsync()

        add({
          title: t('success.backendEnabled'),
          description: t('success.syncStarted'),
          color: 'success',
        })
      } else {
        // Stop sync
        await syncOrchestratorStore.stopSyncAsync()

        add({
          title: t('success.backendDisabled'),
          description: t('success.syncStopped'),
          color: 'success',
        })
      }

      // Refresh server vaults list
      await loadAllServerVaultsAsync()
    } catch (error) {
      console.error('Failed to toggle backend:', error)
      add({
        title: t('errors.toggleFailed'),
        description: error instanceof Error ? error.message : 'Unknown error',
        color: 'error',
      })
    }
  }

  // Delete backend completely (incl. server data)
  const deleteBackendCompletelyAsync = async (
    backend: SelectHaexSyncBackends,
  ) => {
    try {
      // Stop sync if this backend is active
      if (backend.enabled) {
        await syncOrchestratorStore.stopSyncAsync()
      }

      // Delete all server data for this backend
      try {
        {
          const identityStore = useIdentityStore()
          const identityResult = await identityStore.getIdentityByIdAsync(backend.identityId)

          if (identityResult?.privateKey) {
            const identity = { privateKey: identityResult.privateKey, did: identityResult.did }
            // Delete every space this identity OWNS on that server. The route
            // is owner-scoped, not role-scoped: it used to authorize off the
            // cached space_members.capability column, which a member could
            // write, so administering a space was enough to destroy someone
            // else's. It also requires DID-Auth — a UCAN proves only who
            // issued it, not who presented it — hence fetchWithDidAuth here.
            // TODO: add `DeleteAdminSpaces = "delete-admin-spaces"` to DidAuthAction in @haex-space/ucan
            try {
              await fetchWithDidAuth(
                `${backend.homeServerUrl}/spaces/my-admin-spaces`,
                identity.privateKey,
                identity.did,
                'delete-admin-spaces',
                { method: 'DELETE' },
              )
            } catch (e) {
              console.warn('[SYNC] Could not delete admin spaces:', e)
            }
          }
        }

        await syncEngineStore.deleteAllVaultDataAsync(backend.id)
      } catch (e) {
        console.warn(
          '[SYNC] Could not delete server data (may already be cleaned up):',
          e,
        )
      }

      // Delete backend from local DB
      await syncBackendsStore.deleteBackendAsync(backend.id)

      add({
        title: t('success.backendDeleted'),
        color: 'success',
      })

      // Reload backends and vaults
      await syncBackendsStore.loadBackendsAsync()
      await loadAllServerVaultsAsync()

      return true
    } catch (error) {
      console.error('Failed to delete backend:', error)
      add({
        title: t('errors.deleteBackendFailed'),
        description: error instanceof Error ? error.message : 'Unknown error',
        color: 'error',
      })
      return false
    }
  }

  // Delete remote vault for a backend (single space or all)
  const deleteRemoteVaultAsync = async (params: {
    backend: SelectHaexSyncBackends
    spaceId: string
    deleteAll: boolean
  }) => {
    const { backend, spaceId, deleteAll } = params

    try {
      const isCurrentVault = spaceId === currentVaultId.value

      // Step 1: Delete data from server FIRST (while backend store is still available)
      if (deleteAll) {
        await syncEngineStore.deleteAllVaultDataAsync(backend.id)
      } else {
        await syncEngineStore.deleteRemoteVaultAsync(backend.id, spaceId)
      }

      // Step 2: Stop sync if deleting the currently synced vault
      if (isCurrentVault) {
        await syncOrchestratorStore.stopSyncAsync()
      }

      add({
        title: t('success.remoteVaultDeleted'),
        description: t('success.remoteVaultDeletedDescription'),
        color: 'success',
      })

      // Reload backends to update the list
      await syncBackendsStore.loadBackendsAsync()

      // Refresh all server vaults
      await loadAllServerVaultsAsync()

      return true
    } catch (error) {
      console.error('Failed to delete remote vault:', error)
      add({
        title: t('errors.deleteRemoteVaultFailed'),
        description: error instanceof Error ? error.message : 'Unknown error',
        color: 'error',
      })
      return false
    }
  }

  // Re-upload current vault to a backend
  const reUploadVaultAsync = async (backend: SelectHaexSyncBackends) => {
    if (!backend || !currentVaultId.value) return false

    isReUploading.value = true

    try {
      // Get vault key from local DB
      const vaultKey = await syncEngineStore.getSyncKeyFromDbAsync(backend.id)
      if (!vaultKey) {
        throw new Error('Vault key not found locally')
      }

      // Get current vault info
      const { currentVault, currentVaultPassword } = storeToRefs(vaultStore)
      if (!currentVault.value || !currentVaultPassword.value) {
        throw new Error('Vault not opened or password not available')
      }

      // Re-upload vault key to server
      await syncEngineStore.reUploadVaultKeyAsync(
        backend.id,
        currentVaultId.value,
        vaultKey,
        currentVault.value.name,
        currentVaultPassword.value,
      )

      // Push all local data to server
      await syncOrchestratorStore.pushAllDataToBackendAsync(backend.id)

      add({
        title: t('reUpload.success.title'),
        description: t('reUpload.success.description'),
        color: 'success',
      })

      // Refresh server vaults
      await loadAllServerVaultsAsync()

      return true
    } catch (error) {
      console.error('Re-upload failed:', error)
      add({
        title: t('reUpload.error.title'),
        description: error instanceof Error ? error.message : 'Unknown error',
        color: 'error',
      })
      return false
    } finally {
      isReUploading.value = false
    }
  }

  return {
    // State
    groupedServerVaults,
    isReUploading,
    // Computed helpers
    getGroupedVaults,
    // Actions
    loadAllServerVaultsAsync,
    toggleBackendAsync,
    deleteBackendCompletelyAsync,
    deleteRemoteVaultAsync,
    reUploadVaultAsync,
  }
}
