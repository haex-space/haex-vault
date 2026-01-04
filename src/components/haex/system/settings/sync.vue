<template>
  <HaexSystemSettingsLayout :title="t('title')" :description="t('description')">
    <UCard
        v-if="showAddBackendForm"
        class="relative"
      >
        <!-- Loading Overlay -->
        <div
          v-if="isLoading"
          class="absolute inset-0 z-10 flex items-center justify-center bg-base-100/80 backdrop-blur-sm rounded-lg"
        >
          <div class="flex flex-col items-center gap-3">
            <div class="loading loading-spinner loading-lg text-primary" />
            <span class="text-sm text-base-content/70">
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

        <HaexSyncAddBackend
          v-model:email="newBackend.email"
          v-model:password="newBackend.password"
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
              icon="mdi-plus"
              :disabled="isLoading"
              @click="onWizardCompleteAsync"
            >
              <span class="hidden @sm:inline">
                {{ t('actions.add') }}
              </span>
            </UiButton>
          </div>
        </template>
      </UCard>

      <!-- Sync Backends List (merged with Vault Overview) -->
      <UCard v-if="!showAddBackendForm || syncBackends.length">
        <template #header>
          <div class="flex items-center justify-between">
            <div>
              <h3 class="text-lg font-semibold">{{ t('backends.title') }}</h3>
              <p class="text-sm text-gray-500 dark:text-gray-400 mt-1">
                {{ t('backends.description') }}
              </p>
            </div>
            <UButton
              v-if="!showAddBackendForm"
              color="primary"
              icon="i-lucide-plus"
              @click="showAddBackendForm = true"
            >
              <span class="hidden @sm:inline">
                {{ t('actions.add') }}
              </span>
            </UButton>
          </div>
        </template>

        <div
          v-if="syncBackends.length"
          class="space-y-3"
        >
          <HaexSyncBackendItem
            v-for="backend in syncBackends"
            :key="backend.id"
            :backend="backend"
          >
            <template #badges>
              <UBadge
                :color="backend.enabled ? 'success' : 'neutral'"
                variant="subtle"
                size="xs"
              >
                {{ backend.enabled ? t('backends.enabled') : t('backends.disabled') }}
              </UBadge>
              <UBadge
                v-if="getSyncState(backend.id)?.isConnected"
                color="info"
                variant="subtle"
                size="xs"
              >
                {{ t('backends.connected') }}
              </UBadge>
              <UBadge
                v-else-if="getSyncState(backend.id)?.isSyncing"
                color="warning"
                variant="subtle"
                size="xs"
              >
                {{ t('backends.syncing') }}
              </UBadge>
            </template>
            <template #actions>
              <UButton
                                :color="backend.enabled ? 'neutral' : 'primary'"
                @click="toggleBackendAsync(backend.id)"
              >
                {{ backend.enabled ? t('actions.disable') : t('actions.enable') }}
              </UButton>
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
                class="text-center text-red-500 text-sm py-4"
              >
                {{ getGroupedVaults(backend.id)?.error }}
              </div>

              <!-- No vaults -->
              <div
                v-else-if="getGroupedVaults(backend.id)?.vaults.length === 0"
                class="space-y-4"
              >
                <p class="text-center text-gray-500 dark:text-gray-400 text-sm py-4">
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
                class="space-y-2"
              >
                <div
                  v-for="vault in getGroupedVaults(backend.id)?.vaults"
                  :key="vault.vaultId"
                  class="flex flex-col gap-2 p-3 rounded-lg"
                  :class="
                    vault.vaultId === currentVaultId
                      ? 'bg-primary/10 border border-primary/20'
                      : 'bg-gray-50 dark:bg-gray-800/50'
                  "
                >
                  <div class="flex flex-col @xs:flex-row @xs:items-center @xs:justify-between gap-2">
                    <div class="flex-1 min-w-0">
                      <div class="flex items-center gap-2 flex-wrap">
                        <p class="font-medium text-sm truncate">
                          {{
                            vault.decryptedName ||
                            t('vaultOverview.encryptedName')
                          }}
                        </p>
                        <UBadge
                          v-if="vault.vaultId === currentVaultId"
                          color="primary"
                          variant="subtle"
                          size="xs"
                        >
                          {{ t('vaultOverview.currentVault') }}
                        </UBadge>
                      </div>
                      <p
                        class="text-xs text-gray-500 dark:text-gray-400 mt-1"
                      >
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
                        size="lg"
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
          class="text-center py-4 text-gray-500 dark:text-gray-400"
        >
          {{ t('backends.noBackends') }}
        </div>
      </UCard>

      <!-- Sync Configuration -->
      <UCard>
        <template #header>
          <div>
            <h3 class="text-lg font-semibold">
              {{ t('config.title') }}
            </h3>
          </div>
        </template>

        <UTabs
          v-model="activeConfigTab"
          :items="configTabItems"
        >
          <template #content="{ item }">
            <!-- Continuous Sync Settings (Push) -->
            <div
              v-if="item.value === 'continuous'"
              class="pt-4"
            >
              <p class="text-sm text-gray-500 dark:text-gray-400 mb-4">
                {{ t('config.continuous.description') }}
              </p>
              <label class="block text-sm font-medium mb-2">
                {{ t('config.debounce.label') }}
              </label>
              <div class="flex items-center gap-3">
                <UInput
                  v-model.number="continuousDebounceSec"
                  type="number"
                  :min="0.1"
                  :max="30"
                  :step="0.5"
                  class="w-24"
                />
                <span class="text-sm text-gray-500">{{ t('config.units.seconds') }}</span>
              </div>
              <p class="text-xs text-gray-500 dark:text-gray-400 mt-2">
                {{ t('config.debounce.hint') }}
              </p>
              <UButton
                v-if="continuousDebounceSec !== syncConfig.continuousDebounceMs / 1000"
                size="xs"
                class="mt-2"
                @click="saveContinuousDebounceAsync"
              >
                {{ t('config.save') }}
              </UButton>
            </div>

            <!-- Periodic Sync Settings (Pull) -->
            <div
              v-if="item.value === 'periodic'"
              class="pt-4"
            >
              <p class="text-sm text-gray-500 dark:text-gray-400 mb-4">
                {{ t('config.periodic.description') }}
              </p>
              <label class="block text-sm font-medium mb-2">
                {{ t('config.interval.label') }}
              </label>
              <div class="flex items-center gap-3">
                <UInput
                  v-model.number="periodicIntervalMin"
                  type="number"
                  :min="1"
                  :max="60"
                  :step="1"
                  class="w-24"
                />
                <span class="text-sm text-gray-500">{{ t('config.units.minutes') }}</span>
              </div>
              <p class="text-xs text-gray-500 dark:text-gray-400 mt-2">
                {{ t('config.interval.hint') }}
              </p>
              <UButton
                v-if="periodicIntervalMin !== syncConfig.periodicIntervalMs / 60000"
                size="xs"
                class="mt-2"
                @click="savePeriodicIntervalAsync"
              >
                {{ t('config.save') }}
              </UButton>
            </div>
          </template>
        </UTabs>
      </UCard>

    <!-- Delete Remote Vault Confirmation Dialog -->
    <UiDialogConfirm
      v-model:open="showDeleteDialog"
      :title="
        t(
          backendToDelete?.vaultId === currentVaultId
            ? 'deleteCurrentVaultSync.title'
            : 'deleteRemoteVault.title',
        )
      "
      :description="
        t(
          backendToDelete?.vaultId === currentVaultId
            ? 'deleteCurrentVaultSync.description'
            : 'deleteRemoteVault.description',
          { vaultName: backendToDelete?.name },
        )
      "
      confirm-label="Löschen"
      @confirm="onConfirmDeleteRemoteVaultAsync"
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
import type { SelectHaexSyncBackends } from '~/database/schemas'
import {
  decryptString,
  deriveKeyFromPassword,
  base64ToArrayBuffer,
} from '@haex-space/vault-sdk'

const { t } = useI18n()
const { add } = useToast()

const syncBackendsStore = useSyncBackendsStore()
const syncEngineStore = useSyncEngineStore()
const syncOrchestratorStore = useSyncOrchestratorStore()
const syncConfigStore = useSyncConfigStore()
const vaultStore = useVaultStore()

const { backends: syncBackends } = storeToRefs(syncBackendsStore)
const { currentVaultId } = storeToRefs(vaultStore)
const { config: syncConfig } = storeToRefs(syncConfigStore)

// Sync connection composable
const {
  isLoading: isConnectionLoading,
  error: connectionError,
  createConnectionAsync,
} = useCreateSyncConnection()

// Local state
const showAddBackendForm = ref(false)
const isLoading = computed(
  () => isConnectionLoading.value,
)

const newBackend = reactive({
  email: '',
  serverUrl: '',
  password: '',
  id: '',
})

const { serverOptions } = useSyncServerOptions()

// Delete remote vault state
const showDeleteDialog = ref(false)
const backendToDelete = ref<SelectHaexSyncBackends | null>(null)

// Re-upload state
const showReUploadDialog = ref(false)
const isReUploading = ref(false)
const reUploadBackend = ref<SelectHaexSyncBackends | null>(null)

// Server vaults management state - grouped by backend
interface ServerVault {
  vaultId: string
  encryptedVaultName: string
  vaultNameNonce: string
  vaultNameSalt: string
  createdAt: string
  decryptedName?: string
}

interface GroupedServerVaults {
  backend: SelectHaexSyncBackends
  vaults: ServerVault[]
  isLoading: boolean
  error: string | null
  currentVaultMissingOnServer: boolean // true if backend is configured for current vault but vault not found on server
}

const groupedServerVaults = ref<GroupedServerVaults[]>([])

// Helper to get grouped vaults for a specific backend
const getGroupedVaults = (backendId: string) => {
  return groupedServerVaults.value.find((g) => g.backend.id === backendId)
}

// Sync configuration
const activeConfigTab = ref('continuous')
// UI uses seconds for debounce, minutes for interval - convert from ms
const continuousDebounceSec = ref(syncConfig.value.continuousDebounceMs / 1000)
const periodicIntervalMin = ref(syncConfig.value.periodicIntervalMs / 60000)

const configTabItems = computed(() => [
  {
    value: 'continuous',
    label: t('config.continuous.label'),
  },
  {
    value: 'periodic',
    label: t('config.periodic.label'),
  },
])

const saveContinuousDebounceAsync = async () => {
  try {
    // Convert seconds to milliseconds for storage
    await syncConfigStore.saveConfigAsync({
      continuousDebounceMs: Math.round(continuousDebounceSec.value * 1000),
    })
    add({
      color: 'success',
      description: t('config.saveSuccess'),
    })
  } catch (error) {
    console.error('Failed to save debounce setting:', error)
    add({
      color: 'error',
      description: t('config.saveError'),
    })
  }
}

const savePeriodicIntervalAsync = async () => {
  try {
    // Convert minutes to milliseconds for storage
    await syncConfigStore.saveConfigAsync({
      periodicIntervalMs: Math.round(periodicIntervalMin.value * 60000),
    })
    add({
      color: 'success',
      description: t('config.saveSuccess'),
    })
  } catch (error) {
    console.error('Failed to save interval setting:', error)
    add({
      color: 'error',
      description: t('config.saveError'),
    })
  }
}

// Cancel add backend
const cancelAddBackend = () => {
  showAddBackendForm.value = false
  newBackend.email = ''
  newBackend.password = ''
  newBackend.serverUrl = ''
}

// Handle wizard completion
const onWizardCompleteAsync = async () => {
  const backendId = await createConnectionAsync({
    serverUrl: newBackend.serverUrl,
    email: newBackend.email,
    password: newBackend.password,
  })

  if (backendId) {
    // Reload server vaults after sync has started
    await loadAllServerVaultsAsync()

    add({
      title: t('success.backendAdded'),
      color: 'success',
    })

    // Reset form and close
    cancelAddBackend()
  } else if (connectionError.value) {
    // Check if it's a duplicate backend error
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
      // Initialize Supabase client for this backend
      await syncEngineStore.initSupabaseClientAsync(backendId)

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
  } catch (error) {
    console.error('Failed to toggle backend:', error)
    add({
      title: t('errors.toggleFailed'),
      description: error instanceof Error ? error.message : 'Unknown error',
      color: 'error',
    })
  }
}

// Get sync state for a backend
const getSyncState = (backendId: string) => {
  return syncOrchestratorStore.getSyncState(backendId)
}

// Load vaults for a specific backend
const loadVaultsForBackendAsync = async (
  backend: SelectHaexSyncBackends,
): Promise<ServerVault[]> => {
  try {
    // Initialize Supabase client if not already done
    if (!syncEngineStore.supabaseClient) {
      await syncEngineStore.initSupabaseClientAsync(backend.id)
    }

    // Get auth token
    const token = await syncEngineStore.getAuthTokenAsync()
    if (!token) {
      throw new Error('Not authenticated')
    }

    // Fetch vaults from server
    const response = await fetch(`${backend.serverUrl}/sync/vaults`, {
      method: 'GET',
      headers: {
        Authorization: `Bearer ${token}`,
      },
    })

    if (!response.ok) {
      throw new Error('Failed to fetch vaults')
    }

    const data = await response.json()
    const vaults: ServerVault[] = data.vaults

    // Try to decrypt vault names if backend has password
    if (backend.password) {
      for (const vault of vaults) {
        try {
          const salt = base64ToArrayBuffer(vault.vaultNameSalt)
          const derivedKey = await deriveKeyFromPassword(
            backend.password,
            salt,
          )
          const decryptedName = await decryptString(
            vault.encryptedVaultName,
            vault.vaultNameNonce,
            derivedKey,
          )
          vault.decryptedName = decryptedName
        } catch (error) {
          console.error('Failed to decrypt vault name:', vault.vaultId, error)
          // Keep vault in list but without decrypted name
        }
      }
    }

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

  // Initialize grouped vaults structure
  groupedServerVaults.value = syncBackends.value.map((backend) => ({
    backend,
    vaults: [],
    isLoading: true,
    error: null,
    currentVaultMissingOnServer: false,
  }))

  // Load vaults for each backend in parallel
  await Promise.allSettled(
    groupedServerVaults.value.map(async (group) => {
      try {
        const vaults = await loadVaultsForBackendAsync(group.backend)

        // Keep all vaults including the currently opened one
        group.vaults = vaults
        group.isLoading = false

        // Check if this backend is configured for current vault but vault is not on server
        if (group.backend.vaultId === currentVaultId.value) {
          const vaultFoundOnServer = vaults.some(
            (v) => v.vaultId === currentVaultId.value,
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

// Auto-load vaults on mount
onMounted(async () => {
  await loadAllServerVaultsAsync()
})

// Prepare delete server vault
const prepareDeleteServerVault = (
  backend: SelectHaexSyncBackends,
  vault: ServerVault,
) => {
  // Set the vault as backend to delete
  backendToDelete.value = {
    ...backend,
    vaultId: vault.vaultId,
  }
  showDeleteDialog.value = true
}

// Confirm delete remote vault
const onConfirmDeleteRemoteVaultAsync = async () => {
  if (!backendToDelete.value || !backendToDelete.value.vaultId) return

  try {
    const backend = backendToDelete.value
    const vaultId = backend.vaultId

    if (!vaultId) {
      throw new Error('Vault ID is required')
    }

    const isCurrentVault = vaultId === currentVaultId.value

    console.log('[SYNC DELETE]', {
      vaultId,
      currentVaultId: currentVaultId.value,
      isCurrentVault,
      backendId: backend.id,
    })

    // Step 1: Delete remote vault from server FIRST (while backend store is still available)
    console.log('[SYNC DELETE] Deleting remote vault from server...')
    await syncEngineStore.deleteRemoteVaultAsync(backend.id, vaultId)
    console.log('[SYNC DELETE] Remote vault deleted from server')

    // Step 2: If this is the current vault, stop sync and delete local backend
    if (isCurrentVault) {
      console.log('[SYNC DELETE] Stopping sync...')
      await syncOrchestratorStore.stopSyncAsync()

      console.log('[SYNC DELETE] Deleting local backend...', backend.id)
      await syncBackendsStore.deleteBackendAsync(backend.id)
      console.log('[SYNC DELETE] Local backend deleted')

      add({
        title: t('success.syncConnectionDeleted'),
        description: t('success.syncConnectionDeletedDescription'),
        color: 'success',
      })
    } else {
      add({
        title: t('success.remoteVaultDeleted'),
        description: t('success.remoteVaultDeletedDescription'),
        color: 'success',
      })
    }

    // Reload backends to update the list
    console.log('[SYNC DELETE] Reloading backends...')
    await syncBackendsStore.loadBackendsAsync()
    console.log('[SYNC DELETE] Backends reloaded:', syncBackends.value.length)

    // Refresh all server vaults
    await loadAllServerVaultsAsync()

    // Close dialog
    showDeleteDialog.value = false
    backendToDelete.value = null
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
const onConfirmReUploadAsync = async (serverPassword: string) => {
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
      serverPassword,
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
</script>

<i18n lang="yaml">
de:
  title: Synchronisation
  description: Verwalte deine Sync-Backends und Account-Einstellungen
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
  config:
    title: Sync-Konfiguration
    continuous:
      label: Push
      description: Lokale Änderungen werden nach einer kurzen Verzögerung an den Server gesendet.
    periodic:
      label: Fallback-Pull
      description: Änderungen werden normalerweise in Echtzeit empfangen. Der periodische Pull holt verpasste Änderungen nach, falls die Verbindung kurzzeitig unterbrochen war.
    debounce:
      label: Verzögerung
      hint: Wartezeit nach der letzten Änderung, bevor gesendet wird
    interval:
      label: Abruf-Intervall
      hint: Zeitabstand zwischen automatischen Fallback-Abrufen
    units:
      seconds: Sekunden
      minutes: Minuten
    save: Speichern
    saveSuccess: Einstellungen gespeichert
    saveError: Fehler beim Speichern der Einstellungen
  actions:
    add: Hinzufügen
    cancel: Abbrechen
    back: Zurück
    addBackend: Backend hinzufügen
    enable: Aktivieren
    disable: Deaktivieren
    delete: Löschen
    deleteWithSync: Sync löschen
    close: Schließen
    manageServerVaults: Server-Vaults verwalten
  serverOptions:
    localhost: Lokal (localhost:3002)
    custom: Benutzerdefiniert...
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
  success:
    signedIn: Erfolgreich angemeldet
    signedOut: Erfolgreich abgemeldet
    serverUrlUpdated: Server-URL aktualisiert
    backendAdded: Backend hinzugefügt
    backendEnabled: Backend aktiviert
    backendDisabled: Backend deaktiviert
    syncStarted: Synchronisation gestartet
    syncStopped: Synchronisation gestoppt
    remoteVaultDeleted: Remote-Vault gelöscht
    remoteVaultDeletedDescription: Die Remote-Vault wurde erfolgreich vom Server gelöscht
    syncConnectionDeleted: Sync-Verbindung gelöscht
    syncConnectionDeletedDescription: Die Sync-Verbindung wurde getrennt und alle Server-Daten wurden gelöscht
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
    deleteRemoteVaultFailed: Remote-Vault konnte nicht gelöscht werden
    noVaultId: Keine Vault-ID für dieses Backend konfiguriert
    loadServerVaultsFailed: Server-Vaults konnten nicht geladen werden
    backendAlreadyExists: Backend bereits vorhanden
    backendAlreadyExistsDescription: Es besteht bereits eine Verbindung zu {serverUrl}
en:
  title: Synchronization
  description: Manage your sync backends and account settings
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
  config:
    title: Sync Configuration
    continuous:
      label: Push
      description: Local changes are sent to the server after a short delay.
    periodic:
      label: Fallback Pull
      description: Changes are normally received in real-time. The periodic pull catches up on missed changes if the connection was briefly interrupted.
    debounce:
      label: Delay
      hint: Wait time after the last change before sending
    interval:
      label: Fetch Interval
      hint: Time between automatic fallback fetches
    units:
      seconds: seconds
      minutes: minutes
    save: Save
    saveSuccess: Settings saved
    saveError: Error saving settings
  actions:
    add: Add
    cancel: Cancel
    back: Back
    addBackend: Add Backend
    enable: Enable
    disable: Disable
    delete: Delete
    deleteWithSync: Delete Sync
    close: Close
    manageServerVaults: Manage Server Vaults
  serverOptions:
    localhost: Local (localhost:3002)
    custom: Custom...
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
  success:
    signedIn: Successfully signed in
    signedOut: Successfully signed out
    serverUrlUpdated: Server URL updated
    backendAdded: Backend added
    backendEnabled: Backend enabled
    backendDisabled: Backend disabled
    syncStarted: Sync started
    syncStopped: Sync stopped
    remoteVaultDeleted: Remote vault deleted
    remoteVaultDeletedDescription: The remote vault was successfully deleted from the server
    syncConnectionDeleted: Sync connection deleted
    syncConnectionDeletedDescription: The sync connection was disconnected and all server data was deleted
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
    deleteRemoteVaultFailed: Failed to delete remote vault
    noVaultId: No vault ID configured for this backend
    loadServerVaultsFailed: Failed to load server vaults
    backendAlreadyExists: Backend already exists
    backendAlreadyExistsDescription: A connection to {serverUrl} already exists
</i18n>
