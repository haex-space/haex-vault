<template>
  <HaexSystemSettingsLayout
    :title="t('backends.title')"
    :description="t('backends.description')"
    show-back
    @back="$emit('back')"
  >
    <template #actions>
      <UButton
        v-if="!showAddBackendForm"
        color="primary"
        icon="i-lucide-plus"
        data-testid="sync-add-backend-button"
        data-tour="settings-sync-add-backend"
        @click="showAddBackendForm = true"
      >
        <span class="hidden @sm:inline">
          {{ t('actions.add') }}
        </span>
      </UButton>
    </template>

    <UCard
      v-if="showAddBackendForm"
      class="relative"
    >
      <!-- Loading Overlay -->
      <div
        v-if="isLoading"
        class="absolute inset-0 z-10 flex items-center justify-center bg-default/80 backdrop-blur-sm rounded-lg"
      >
        <div class="flex flex-col items-center gap-3">
          <div class="loading loading-spinner loading-lg text-primary" />
          <span class="text-sm text-muted">
            {{ t('addBackend.connecting') }}
          </span>
        </div>
      </div>

      <template #header>
        <div class="flex justify-between px-1">
          <h3 class="text-lg font-semibold">
            {{ t('addBackend.title') }}
          </h3>

          <UiButton
            icon="mdi-close"
            variant="ghost"
            color="neutral"
            :disabled="isLoading"
            @click="showAddBackendForm = false"
          />
        </div>
      </template>

      <!-- Verification Code Input -->
      <div
        v-if="verificationPending"
        class="space-y-4"
      >
        <UAlert
          color="info"
          icon="i-lucide-mail"
          :title="t('verification.title')"
          :description="t('verification.description')"
        />

        <div class="flex justify-center">
          <UPinInput
            v-model="verificationCodeParts"
            :length="6"
            otp
            type="number"
            size="xl"
            :autofocus="true"
            :ui="{ base: 'w-12 h-12 text-center text-lg' }"
            @complete="onVerifyCodeAsync"
          />
        </div>

        <UButton
          variant="link"
          :disabled="isLoading"
          @click="onResendCodeAsync"
        >
          {{ t('verification.resend') }}
        </UButton>
      </div>

      <!-- Add Backend Form -->
      <HaexSyncAddBackend
        v-else
        v-model:identity-id="newBackend.identityId"
        v-model:approved-claims="newBackend.approvedClaims"
        v-model:server-url="newBackend.serverUrl"
        :items="serverOptions"
        @keydown.enter.prevent="onWizardCompleteAsync"
      />

      <template #footer>
        <div class="flex justify-between">
          <UButton
            color="neutral"
            variant="outline"
            :disabled="isLoading"
            @click="cancelAddBackend"
          >
            {{ t('actions.back') }}
          </UButton>

          <UiButton
            v-if="verificationPending"
            icon="mdi-check"
            :disabled="isLoading || verificationCode.length !== 6"
            @click="onVerifyCodeAsync"
          >
            <span class="hidden @sm:inline">
              {{ t('verification.verify') }}
            </span>
          </UiButton>
          <UiButton
            v-else
            icon="mdi-plus"
            :disabled="isLoading"
            data-testid="sync-submit-button"
            @click="onWizardCompleteAsync"
          >
            <span class="hidden @sm:inline">
              {{ t('actions.add') }}
            </span>
          </UiButton>
        </div>
      </template>
    </UCard>

    <!-- Sync Backends List -->
    <div>
      <div
        v-if="syncBackends.length"
        class="space-y-3"
      >
        <HaexSyncBackendItem
          v-for="backend in syncBackends"
          :key="backend.id"
          :backend="backend"
        >
          <template #actions>
            <div class="flex gap-2">
              <UButton
                :color="backend.enabled ? 'neutral' : 'primary'"
                icon="i-lucide-power"
                :title="
                  backend.enabled ? t('actions.disable') : t('actions.enable')
                "
                @click="toggleBackendAsync(backend.id)"
              >
                {{
                  backend.enabled ? t('actions.disable') : t('actions.enable')
                }}
              </UButton>
              <UButton
                color="error"
                variant="ghost"
                icon="i-lucide-trash-2"
                :title="t('actions.deleteBackend')"
                @click="prepareDeleteBackend(backend)"
              />
            </div>
          </template>

          <!-- Server Vaults for this backend -->
          <template
            v-if="getGroupedVaults(backend.id)"
            #default
          >
            <!-- Loading state -->
            <div
              v-if="getGroupedVaults(backend.id)?.isLoading"
              class="flex items-center justify-center py-4"
            >
              <UIcon
                name="i-lucide-loader-2"
                class="w-5 h-5 animate-spin text-primary"
              />
            </div>

            <!-- Error state -->
            <div
              v-else-if="getGroupedVaults(backend.id)?.error"
              class="text-center text-error text-sm py-4"
            >
              {{ getGroupedVaults(backend.id)?.error }}
            </div>

            <!-- No vaults -->
            <div
              v-else-if="getGroupedVaults(backend.id)?.vaults.length === 0"
              class="space-y-4"
            >
              <p
                class="text-center text-muted text-sm py-4"
              >
                {{ t('vaultOverview.noVaults') }}
              </p>

              <!-- Re-Upload option when current vault is missing on server -->
              <div
                v-if="getGroupedVaults(backend.id)?.currentVaultMissingOnServer"
                class="space-y-3"
              >
                <UAlert
                  color="warning"
                  icon="i-lucide-alert-triangle"
                  :title="t('reUpload.warning.title')"
                  :description="t('reUpload.warning.description')"
                />
                <div class="flex justify-end">
                  <UButton
                    color="primary"
                    icon="i-lucide-upload"
                    :loading="isReUploading"
                    :disabled="isReUploading"
                    @click="prepareReUpload(backend)"
                  >
                    {{ t('reUpload.button') }}
                  </UButton>
                </div>
              </div>
            </div>

            <!-- Vaults list -->
            <div
              v-else
              class="divide-y divide-default"
            >
              <div
                v-for="vault in getGroupedVaults(backend.id)?.vaults"
                :key="vault.spaceId"
                class="flex flex-col gap-2 py-5 px-3"
                :class="
                  vault.spaceId === currentVaultId
                    ? 'bg-primary/10  rounded-lg  border border-primary/20'
                    : ''
                "
              >
                <div
                  class="flex flex-col @xs:flex-row @xs:items-center @xs:justify-between gap-2"
                >
                  <div class="flex-1 min-w-0">
                    <div class="flex items-center gap-2 flex-wrap">
                      <p class="font-medium text-base truncate">
                        {{
                          vault.decryptedName ||
                          t('vaultOverview.encryptedName')
                        }}
                      </p>
                      <UBadge
                        v-if="vault.spaceId === currentVaultId"
                        color="primary"
                        variant="subtle"
                      >
                        {{ t('vaultOverview.currentVault') }}
                      </UBadge>
                    </div>
                    <p class="text-sm text-muted mt-1">
                      {{ t('vaultOverview.createdAt') }}:
                      {{ formatDate(vault.createdAt) }}
                    </p>
                  </div>
                  <!-- Delete button -->
                  <div class="@xs:shrink-0 w-full @xs:w-auto">
                    <UButton
                      color="error"
                      variant="ghost"
                      icon="i-lucide-trash-2"
                      class="w-full @xs:w-auto justify-center"
                      @click="prepareDeleteServerVault(backend, vault)"
                    />
                  </div>
                </div>
              </div>
            </div>
          </template>
        </HaexSyncBackendItem>
      </div>

      <div
        v-else
        class="text-center py-4 text-muted"
      >
        {{ t('backends.noBackends') }}
      </div>
    </div>

    <!-- Delete Remote Vault Confirmation Dialog -->
    <UiDialogConfirm
      v-model:open="showDeleteDialog"
      :title="
        t(
          vaultToDeleteSpaceId === currentVaultId
            ? 'deleteCurrentVaultSync.title'
            : 'deleteRemoteVault.title',
        )
      "
      :description="
        t(
          vaultToDeleteSpaceId === currentVaultId
            ? 'deleteCurrentVaultSync.description'
            : 'deleteRemoteVault.description',
          { vaultName: vaultToDeleteName },
        )
      "
      :confirm-label="t('actions.delete')"
      @confirm="onConfirmDeleteRemoteVaultAsync"
    >
      <template #body>
        <label
          class="flex items-start gap-3 cursor-pointer mt-4 p-3 rounded-lg border border-error/30 bg-error/5"
        >
          <UCheckbox
            v-model="deleteAllServerData"
            color="error"
          />
          <div>
            <p class="text-sm font-medium text-error">
              {{ t('deleteAllData.label') }}
            </p>
            <p class="text-xs text-muted mt-0.5">
              {{ t('deleteAllData.description') }}
            </p>
          </div>
        </label>
      </template>
    </UiDialogConfirm>

    <!-- Delete Backend Confirmation Dialog -->
    <UiDialogConfirm
      v-model:open="showDeleteBackendDialog"
      :title="t('deleteBackend.title')"
      :description="
        t('deleteBackend.description', {
          name: backendToDeleteCompletely?.name,
        })
      "
      :confirm-label="t('actions.delete')"
      @confirm="onConfirmDeleteBackendAsync"
    />

    <!-- Re-Upload Confirmation Dialog -->
    <HaexSyncReUploadDialog
      v-model:open="showReUploadDialog"
      :backend="reUploadBackend"
      :loading="isReUploading"
      @confirm="onConfirmReUploadAsync"
    />
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { DidAuthAction } from '@haex-space/ucan'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { decryptVaultNameAsync } from '@/utils/crypto/vaultName'
import type { SelectHaexSyncBackends } from '~/database/schemas'

defineEmits<{ back: [] }>()

const { t } = useI18n()
const { add } = useToast()

const syncBackendsStore = useSyncBackendsStore()
const syncEngineStore = useSyncEngineStore()
const syncOrchestratorStore = useSyncOrchestratorStore()
const vaultStore = useVaultStore()

const { backends: syncBackends } = storeToRefs(syncBackendsStore)
const { currentVaultId } = storeToRefs(vaultStore)

// Sync connection composable
const {
  isLoading,
  error: connectionError,
  createConnectionAsync,
  verifyEmailAsync,
  resendVerificationAsync,
  completeConnectionAsync,
} = useCreateSyncConnection()

// Local state
const showAddBackendForm = ref(false)

const newBackend = reactive({
  serverUrl: '',
  identityId: '',
  approvedClaims: {} as Record<string, string>,
})

// Verification state
const verificationPending = ref<{
  did: string
  serverUrl: string
  identityId: string
  approvedClaims: Record<string, string>
} | null>(null)
const verificationCodeParts = ref<number[]>([])
const verificationCode = computed(() => verificationCodeParts.value.join(''))

const { serverOptions } = useSyncServerOptions()

// Delete remote vault state
const showDeleteDialog = ref(false)
const backendToDelete = ref<SelectHaexSyncBackends | null>(null)
const vaultToDeleteSpaceId = ref<string | null>(null)
const vaultToDeleteName = ref<string | null>(null)
const deleteAllServerData = ref(false)

// Delete backend state
const showDeleteBackendDialog = ref(false)
const backendToDeleteCompletely = ref<SelectHaexSyncBackends | null>(null)

// Re-upload state
const showReUploadDialog = ref(false)
const isReUploading = ref(false)
const reUploadBackend = ref<SelectHaexSyncBackends | null>(null)

// Server vaults management state - grouped by backend
interface ServerVault {
  spaceId: string
  encryptedVaultName: string
  vaultNameNonce: string
  vaultNameSalt: string
  ephemeralPublicKey: string
  createdAt: string
  decryptedName?: string
}

interface GroupedServerVaults {
  backend: SelectHaexSyncBackends
  vaults: ServerVault[]
  isLoading: boolean
  error: string | null
  currentVaultMissingOnServer: boolean
}

const groupedServerVaults = ref<GroupedServerVaults[]>([])

// Helper to get grouped vaults for a specific backend
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

// Cancel add backend
const cancelAddBackend = () => {
  showAddBackendForm.value = false
  newBackend.serverUrl = ''
  newBackend.identityId = ''
  newBackend.approvedClaims = {}
  verificationPending.value = null
  verificationCodeParts.value = []
}

// Handle wizard completion
const onWizardCompleteAsync = async () => {
  const result = await createConnectionAsync({
    serverUrl: newBackend.serverUrl,
    identityId: newBackend.identityId,
    approvedClaims: newBackend.approvedClaims,
  })

  if (!result) {
    if (connectionError.value) {
      if (connectionError.value.includes('already exists')) {
        add({
          title: t('errors.backendAlreadyExists'),
          description: t('errors.backendAlreadyExistsDescription', {
            serverUrl: newBackend.serverUrl,
          }),
          color: 'warning',
        })
      } else {
        add({
          title: t('errors.addBackendFailed'),
          description: connectionError.value,
          color: 'error',
        })
      }
    }
    return
  }

  if (result.status === 'verification_pending') {
    verificationPending.value = {
      did: result.did,
      serverUrl: result.serverUrl,
      identityId: result.identityId,
      approvedClaims: result.approvedClaims,
    }
    add({
      title: t('verification.codeSent'),
      description: t('verification.checkEmail'),
      color: 'info',
    })
    return
  }

  // Connected successfully
  await loadAllServerVaultsAsync()
  add({ title: t('success.backendAdded'), color: 'success' })
  cancelAddBackend()
}

// Handle OTP verification
const onVerifyCodeAsync = async () => {
  if (!verificationPending.value || !verificationCode.value) return

  const { did, serverUrl, identityId } = verificationPending.value

  const verified = await verifyEmailAsync(
    serverUrl,
    did,
    verificationCode.value,
  )
  if (!verified) {
    add({
      title: t('verification.failed'),
      description: connectionError.value || '',
      color: 'error',
    })
    return
  }

  // Verification succeeded — complete the connection
  const backendId = await completeConnectionAsync({ serverUrl, identityId })

  if (backendId) {
    await loadAllServerVaultsAsync()
    add({ title: t('success.backendAdded'), color: 'success' })
    cancelAddBackend()
  } else if (connectionError.value) {
    add({
      title: t('errors.addBackendFailed'),
      description: connectionError.value,
      color: 'error',
    })
  }
}

// Resend verification code
const onResendCodeAsync = async () => {
  if (!verificationPending.value) return
  const { serverUrl, did } = verificationPending.value
  const sent = await resendVerificationAsync(serverUrl, did)
  if (sent) {
    add({
      title: t('verification.codeResent'),
      color: 'success',
    })
  } else {
    add({
      title: t('verification.resendFailed'),
      description: connectionError.value || '',
      color: 'error',
    })
  }
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

// Prepare delete backend
const prepareDeleteBackend = (backend: SelectHaexSyncBackends) => {
  backendToDeleteCompletely.value = backend
  showDeleteBackendDialog.value = true
}

// Confirm delete backend
const onConfirmDeleteBackendAsync = async () => {
  const backend = backendToDeleteCompletely.value
  if (!backend) return

  try {
    // Stop sync if this backend is active
    if (backend.enabled) {
      await syncOrchestratorStore.stopSyncAsync()
    }

    // Delete all server data for this backend
    try {
      if (backend.identityId) {
        const identityStore = useIdentityStore()
        const identity = await identityStore.getIdentityAsync(backend.identityId)

        if (identity?.privateKey && identity?.did) {
          // Delete all spaces where user is admin (server validates role)
          try {
            await fetchWithDidAuth(
              `${backend.serverUrl}/spaces/my-admin-spaces`,
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

    showDeleteBackendDialog.value = false
    backendToDeleteCompletely.value = null
  } catch (error) {
    console.error('Failed to delete backend:', error)
    add({
      title: t('errors.deleteBackendFailed'),
      description: error instanceof Error ? error.message : 'Unknown error',
      color: 'error',
    })
  }
}

// Load vaults for a specific backend
const loadVaultsForBackendAsync = async (
  backend: SelectHaexSyncBackends,
): Promise<ServerVault[]> => {
  try {
    if (!backend.identityId) {
      throw new Error('Backend has no identity configured')
    }

    const identityStore = useIdentityStore()
    const identity = await identityStore.getIdentityAsync(backend.identityId)
    if (!identity?.privateKey || !identity?.did) {
      throw new Error('Identity not found or incomplete')
    }

    // Fetch vaults from server using DID-Auth
    const response = await fetchWithDidAuth(
      `${backend.serverUrl}/sync/vaults`,
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

// Prepare delete server vault
const prepareDeleteServerVault = (
  backend: SelectHaexSyncBackends,
  vault: ServerVault,
) => {
  backendToDelete.value = backend
  vaultToDeleteSpaceId.value = vault.spaceId
  vaultToDeleteName.value = vault.decryptedName || t('vaultOverview.encryptedName')
  deleteAllServerData.value = false
  showDeleteDialog.value = true
}

// Confirm delete remote vault
const onConfirmDeleteRemoteVaultAsync = async () => {
  const backend = backendToDelete.value
  const spaceId = vaultToDeleteSpaceId.value
  if (!backend || !spaceId) return

  try {
    const isCurrentVault = spaceId === currentVaultId.value

    // Step 1: Delete data from server FIRST (while backend store is still available)
    if (deleteAllServerData.value) {
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

    // Close dialog and reset state
    showDeleteDialog.value = false
    backendToDelete.value = null
    vaultToDeleteSpaceId.value = null
    vaultToDeleteName.value = null
    deleteAllServerData.value = false
  } catch (error) {
    console.error('Failed to delete remote vault:', error)
    add({
      title: t('errors.deleteRemoteVaultFailed'),
      description: error instanceof Error ? error.message : 'Unknown error',
      color: 'error',
    })
  }
}

// Format date helper
const formatDate = (dateStr: string) => {
  return new Date(dateStr).toLocaleDateString()
}

// Prepare re-upload for a specific backend
const prepareReUpload = (backend: SelectHaexSyncBackends) => {
  reUploadBackend.value = backend
  showReUploadDialog.value = true
}

// Confirm re-upload
const onConfirmReUploadAsync = async () => {
  const backend = reUploadBackend.value
  if (!backend || !currentVaultId.value) return

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

    showReUploadDialog.value = false
    reUploadBackend.value = null
  } catch (error) {
    console.error('Re-upload failed:', error)
    add({
      title: t('reUpload.error.title'),
      description: error instanceof Error ? error.message : 'Unknown error',
      color: 'error',
    })
  } finally {
    isReUploading.value = false
  }
}

// Auto-load vaults on mount
onMounted(async () => {
  await loadAllServerVaultsAsync()
})
</script>

<i18n lang="yaml">
de:
  addBackend:
    title: Backend hinzufügen
    connecting: Verbindung wird hergestellt...
  backends:
    title: Sync-Backends
    description: Verbundene Server für die Synchronisation deiner Daten
    noBackends: Keine Sync-Backends konfiguriert
    enabled: Aktiviert
    disabled: Deaktiviert
    connected: Verbunden
    syncing: Synchronisiert
  actions:
    add: Hinzufügen
    cancel: Abbrechen
    back: Zurück
    addBackend: Backend hinzufügen
    enable: Aktivieren
    disable: Deaktivieren
    delete: Löschen
    deleteBackend: Backend löschen
    deleteWithSync: Sync löschen
    close: Schließen
    manageServerVaults: Server-Vaults verwalten
  vaultOverview:
    encryptedName: Verschlüsselter Name
    createdAt: Erstellt am
    noVaults: Keine Vaults auf dem Server gefunden
    loadError: Fehler beim Laden
    currentVault: Aktuell geöffnet
  deleteRemoteVault:
    title: Remote-Vault löschen
    description: Möchtest du die Remote-Vault "{vaultName}" wirklich vom Server löschen? Diese Aktion kann nicht rückgängig gemacht werden. Alle Daten dieser Vault werden vom Server entfernt.
  deleteCurrentVaultSync:
    title: Sync-Verbindung löschen
    description: Möchtest du die Sync-Verbindung für die aktuell geöffnete Vault wirklich löschen? Alle Daten dieser Vault werden vom Server "{vaultName}" entfernt und die Sync-Verbindung wird getrennt. Deine lokalen Daten bleiben erhalten.
  deleteBackend:
    title: Backend löschen
    description: Möchtest du das Backend "{name}" wirklich löschen? Alle Vault-Daten auf dem Server und die lokale Verbindung werden entfernt. Diese Aktion kann nicht rückgängig gemacht werden.
  deleteAllData:
    label: Alle Vault-Daten auf dem Server löschen
    description: Löscht sämtliche Vault-Daten auf diesem Server (alle Vaults und Sync-Daten). Dein Account bleibt bestehen. Diese Aktion kann nicht rückgängig gemacht werden.
  success:
    signedIn: Erfolgreich angemeldet
    signedOut: Erfolgreich abgemeldet
    serverUrlUpdated: Server-URL aktualisiert
    backendAdded: Backend hinzugefügt
    backendEnabled: Backend aktiviert
    backendDisabled: Backend deaktiviert
    syncStarted: Synchronisation gestartet
    syncStopped: Synchronisation gestoppt
    backendDeleted: Backend gelöscht
    remoteVaultDeleted: Remote-Vault gelöscht
    remoteVaultDeletedDescription: Die Remote-Vault wurde erfolgreich vom Server gelöscht
    syncConnectionDeleted: Sync-Verbindung gelöscht
    syncConnectionDeletedDescription: Die Sync-Verbindung wurde getrennt und alle Server-Daten wurden gelöscht
  verification:
    title: E-Mail-Verifizierung
    description: Ein 6-stelliger Bestätigungscode wurde an deine E-Mail gesendet. Gib den Code ein, um dein Konto zu verifizieren.
    placeholder: '000000'
    verify: Verifizieren
    resend: Code erneut senden
    codeSent: Code gesendet
    checkEmail: Prüfe dein E-Mail-Postfach für den Bestätigungscode.
    codeResent: Code erneut gesendet
    failed: Verifizierung fehlgeschlagen
    resendFailed: Code konnte nicht erneut gesendet werden
  reUpload:
    warning:
      title: Vault nicht auf Server gefunden
      description: Die aktuell geöffnete Vault wurde auf diesem Server nicht gefunden. Du kannst alle lokalen Daten erneut hochladen.
    button: Daten hochladen
    success:
      title: Daten hochgeladen
      description: Alle lokalen Daten wurden erfolgreich auf den Server hochgeladen.
    error:
      title: Upload fehlgeschlagen
  errors:
    noBackend: Kein Backend konfiguriert
    noServerUrl: Bitte trage zuerst die Server-URL ein
    initFailed: Initialisierung fehlgeschlagen
    signInFailed: Anmeldung fehlgeschlagen
    signOutFailed: Abmeldung fehlgeschlagen
    addBackendFailed: Backend konnte nicht hinzugefügt werden
    toggleFailed: Status konnte nicht geändert werden
    deleteBackendFailed: Backend konnte nicht gelöscht werden
    deleteRemoteVaultFailed: Remote-Vault konnte nicht gelöscht werden
    noVaultId: Keine Vault-ID für dieses Backend konfiguriert
    loadServerVaultsFailed: Server-Vaults konnten nicht geladen werden
    backendAlreadyExists: Backend bereits vorhanden
    backendAlreadyExistsDescription: Es besteht bereits eine Verbindung zu {serverUrl}
en:
  addBackend:
    title: Add Backend
    connecting: Connecting...
  backends:
    title: Sync Backends
    description: Connected servers for syncing your data
    noBackends: No sync backends configured
    enabled: Enabled
    disabled: Disabled
    connected: Connected
    syncing: Syncing
  actions:
    add: Add
    cancel: Cancel
    back: Back
    addBackend: Add Backend
    enable: Enable
    disable: Disable
    delete: Delete
    deleteBackend: Delete backend
    deleteWithSync: Delete Sync
    close: Close
    manageServerVaults: Manage Server Vaults
  vaultOverview:
    title: Vault Overview
    description: Here you can see all vaults stored on the servers. You can delete orphaned vaults that you no longer have local access to.
    encryptedName: Encrypted Name
    createdAt: Created at
    noVaults: No vaults found on server
    loadError: Error loading
    currentVault: Currently opened
  deleteRemoteVault:
    title: Delete Remote Vault
    description: Do you really want to delete the remote vault "{vaultName}" from the server? This action cannot be undone. All data of this vault will be removed from the server.
  deleteCurrentVaultSync:
    title: Delete Sync Connection
    description: Do you really want to delete the sync connection for the currently opened vault? All data of this vault will be removed from the server "{vaultName}" and the sync connection will be disconnected. Your local data will remain intact.
  deleteBackend:
    title: Delete Backend
    description: Do you really want to delete the backend "{name}"? All vault data on the server and the local connection will be removed. This action cannot be undone.
  deleteAllData:
    label: Delete all vault data on the server
    description: Deletes all vault data on this server (all vaults and sync data). Your account remains intact. This action cannot be undone.
  success:
    signedIn: Successfully signed in
    signedOut: Successfully signed out
    serverUrlUpdated: Server URL updated
    backendAdded: Backend added
    backendEnabled: Backend enabled
    backendDisabled: Backend disabled
    syncStarted: Sync started
    syncStopped: Sync stopped
    backendDeleted: Backend deleted
    remoteVaultDeleted: Remote vault deleted
    remoteVaultDeletedDescription: The remote vault was successfully deleted from the server
    syncConnectionDeleted: Sync connection deleted
    syncConnectionDeletedDescription: The sync connection was disconnected and all server data was deleted
  verification:
    title: Email Verification
    description: A 6-digit verification code was sent to your email. Enter the code to verify your account.
    placeholder: '000000'
    verify: Verify
    resend: Resend code
    codeSent: Code sent
    checkEmail: Check your email inbox for the verification code.
    codeResent: Code resent
    failed: Verification failed
    resendFailed: Could not resend code
  reUpload:
    warning:
      title: Vault not found on server
      description: The currently opened vault was not found on this server. You can re-upload all local data.
    button: Upload Data
    success:
      title: Data uploaded
      description: All local data was successfully uploaded to the server.
    error:
      title: Upload failed
  errors:
    noBackend: No backend configured
    noServerUrl: Please enter the server URL first
    initFailed: Initialization failed
    signInFailed: Sign in failed
    signOutFailed: Sign out failed
    addBackendFailed: Failed to add backend
    toggleFailed: Failed to toggle status
    deleteBackendFailed: Failed to delete backend
    deleteRemoteVaultFailed: Failed to delete remote vault
    noVaultId: No vault ID configured for this backend
    loadServerVaultsFailed: Failed to load server vaults
    backendAlreadyExists: Backend already exists
    backendAlreadyExistsDescription: A connection to {serverUrl} already exists
</i18n>
