<template>
  <div>
    <div class="p-6 border-b border-base-content/10">
      <h2 class="text-2xl font-bold">
        {{ t('title') }}
      </h2>
      <p class="text-sm text-base-content/60 mt-1">
        {{ t('description') }}
      </p>
    </div>

    <div class="@container p-6 space-y-6">
      <UCard v-if="showAddBackendForm">
        <template #header>
          <div class="flex justify-between px-1">
            <h3 class="text-lg font-semibold">
              {{ t('addBackend.title') }}
            </h3>

            <UiButton
              icon="mdi-close"
              variant="ghost"
              color="neutral"
              @click="showAddBackendForm = false"
            />
          </div>
        </template>

        <HaexSyncAddBackend
          v-model:email="newBackend.email"
          v-model:password="newBackend.password"
          v-model:server-url="newBackend.serverUrl"
          :items="serverOptions"
        />

        <template #footer>
          <div class="flex justify-between">
            <UButton
              color="neutral"
              variant="outline"
              @click="cancelAddBackend"
            >
              {{ t('actions.back') }}
            </UButton>

            <UiButton
              icon="mdi-plus"
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
      <UCard>
        <template #header>
          <div class="flex items-center justify-between">
            <h3 class="text-lg font-semibold">{{ t('backends.title') }}</h3>
            <UButton
              color="primary"
              size="sm"
              icon="i-lucide-plus"
              @click="showAddBackendForm = true"
            >
              <span class="hidden @sm:inline">
                {{ t('actions.add') }}
              </span>
            </UButton>
          </div>
        </template>

        <template
          v-if="syncBackends.length"
          #default
        >
          <div class="space-y-3">
            <div
              v-for="backend in syncBackends"
              :key="backend.id"
              class="p-4 bg-gray-50 dark:bg-gray-800/50 rounded-lg"
            >
              <div
                class="flex flex-col @sm:flex-row @sm:items-center justify-between gap-3"
              >
                <div class="flex-1 min-w-0">
                  <p class="font-medium">{{ backend.name }}</p>
                  <p class="text-sm text-gray-500 dark:text-gray-400 truncate">
                    {{ backend.serverUrl }}
                  </p>
                  <div class="flex flex-wrap gap-2 mt-2">
                    <UBadge
                      :color="backend.enabled ? 'success' : 'neutral'"
                      variant="subtle"
                      size="xs"
                    >
                      {{
                        backend.enabled
                          ? t('backends.enabled')
                          : t('backends.disabled')
                      }}
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
                  </div>
                </div>
                <div class="shrink-0">
                  <UButton
                    size="sm"
                    :color="backend.enabled ? 'neutral' : 'primary'"
                    class="w-full @sm:w-auto"
                    @click="toggleBackendAsync(backend.id)"
                  >
                    {{
                      backend.enabled
                        ? t('actions.disable')
                        : t('actions.enable')
                    }}
                  </UButton>
                </div>
              </div>
            </div>
          </div>
        </template>
      </UCard>

      <!-- Sync Configuration -->
      <UCard>
        <template #header>
          <div>
            <h3 class="text-lg font-semibold">
              {{ t('config.title') }}
            </h3>
            <p class="text-sm text-gray-500 dark:text-gray-400 mt-1">
              {{ t('config.description') }}
            </p>
          </div>
        </template>

        <div class="space-y-6">
          <!-- Sync Mode Selection -->
          <div>
            <label class="block text-sm font-medium mb-2">
              {{ t('config.mode.label') }}
            </label>
            <URadioGroup
              v-model="syncMode"
              :options="syncModeOptions"
              @update:model-value="onSyncModeChangeAsync"
            />
            <p class="text-xs text-gray-500 dark:text-gray-400 mt-2">
              {{
                syncMode === 'continuous'
                  ? t('config.mode.continuous.description')
                  : t('config.mode.periodic.description')
              }}
            </p>
          </div>

          <!-- Continuous Mode Settings -->
          <div v-if="syncMode === 'continuous'">
            <label class="block text-sm font-medium mb-2">
              {{ t('config.debounce.label') }}
            </label>
            <div class="flex items-center gap-3">
              <UInput
                v-model.number="continuousDebounceMs"
                type="number"
                :min="100"
                :max="10000"
                :step="100"
                class="w-32"
              />
              <span class="text-sm text-gray-500">ms</span>
            </div>
            <p class="text-xs text-gray-500 dark:text-gray-400 mt-2">
              {{ t('config.debounce.description') }}
            </p>
            <UButton
              v-if="continuousDebounceMs !== syncConfig.continuousDebounceMs"
              size="xs"
              class="mt-2"
              @click="saveContinuousDebounceAsync"
            >
              {{ t('config.save') }}
            </UButton>
          </div>

          <!-- Periodic Mode Settings -->
          <div v-if="syncMode === 'periodic'">
            <label class="block text-sm font-medium mb-2">
              {{ t('config.interval.label') }}
            </label>
            <div class="flex items-center gap-3">
              <UInput
                v-model.number="periodicIntervalMs"
                type="number"
                :min="5000"
                :max="3600000"
                :step="1000"
                class="w-32"
              />
              <span class="text-sm text-gray-500">ms</span>
              <span class="text-sm text-gray-500"
                >({{ Math.round(periodicIntervalMs / 1000) }}s)</span
              >
            </div>
            <p class="text-xs text-gray-500 dark:text-gray-400 mt-2">
              {{ t('config.interval.description') }}
            </p>
            <UButton
              v-if="periodicIntervalMs !== syncConfig.periodicIntervalMs"
              size="xs"
              class="mt-2"
              @click="savePeriodicIntervalAsync"
            >
              {{ t('config.save') }}
            </UButton>
          </div>
        </div>
      </UCard>

      <!-- Vault Overview with Accordions -->
      <UCard>
        <template #header>
          <div>
            <h3 class="text-lg font-semibold">
              {{ t('vaultOverview.title') }}
            </h3>
            <p class="text-sm text-gray-500 dark:text-gray-400 mt-1">
              {{ t('vaultOverview.description') }}
            </p>
          </div>
        </template>

        <!-- Overall loading state -->
        <template
          v-if="groupedServerVaults.length"
          #default
        >
          <div
            v-if="isLoadingAllServerVaults"
            class="flex items-center justify-center py-8"
          >
            <UIcon
              name="i-lucide-loader-2"
              class="w-8 h-8 animate-spin text-primary"
            />
          </div>

          <!-- Accordion per backend -->
          <div
            v-else
            class="space-y-2"
          >
            <div
              v-for="group in groupedServerVaults"
              :key="group.backend.id"
            >
              <UAccordion
                :items="[
                  {
                    label: group.backend.name,
                    slot: 'content',
                  },
                ]"
              >
                <template #default>
                  <div class="flex items-center justify-between w-full">
                    <div class="flex items-center gap-3 flex-1 min-w-0">
                      <div class="flex-1 min-w-0">
                        <p class="font-medium truncate">
                          {{ group.backend.name }}
                        </p>
                        <p
                          class="text-xs text-gray-500 dark:text-gray-400 truncate"
                        >
                          {{ group.backend.serverUrl }}
                        </p>
                      </div>
                    </div>
                    <div class="flex items-center gap-2 ml-3">
                      <UIcon
                        v-if="group.isLoading"
                        name="i-lucide-loader-2"
                        class="w-4 h-4 animate-spin"
                      />
                      <UBadge
                        v-else-if="group.error"
                        color="error"
                        variant="subtle"
                        size="xs"
                      >
                        {{ t('vaultOverview.loadError') }}
                      </UBadge>
                      <UBadge
                        v-else
                        color="neutral"
                        variant="subtle"
                        size="xs"
                      >
                        {{ group.vaults.length }}
                      </UBadge>
                    </div>
                  </div>
                </template>

                <template #content>
                  <!-- Error state -->
                  <div
                    v-if="group.error"
                    class="text-center text-red-500 text-sm py-4"
                  >
                    {{ group.error }}
                  </div>

                  <!-- No vaults -->
                  <div
                    v-else-if="group.vaults.length === 0"
                    class="text-center text-gray-500 dark:text-gray-400 text-sm py-4"
                  >
                    {{ t('vaultOverview.noVaults') }}
                  </div>

                  <!-- Vaults list -->
                  <div
                    v-else
                    class="space-y-2"
                  >
                    <div
                      v-for="vault in group.vaults"
                      :key="vault.vaultId"
                      class="flex items-center justify-between p-3 rounded-lg"
                      :class="
                        vault.vaultId === currentVaultId
                          ? 'bg-primary/10 border border-primary/20'
                          : 'bg-gray-50 dark:bg-gray-800/50'
                      "
                    >
                      <div class="flex-1 min-w-0 flex items-center gap-2">
                        <div class="flex-1 min-w-0">
                          <div class="flex items-center gap-2">
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
                      </div>
                      <UButton
                        size="xs"
                        color="error"
                        variant="ghost"
                        icon="i-lucide-trash-2"
                        @click="prepareDeleteServerVault(group.backend, vault)"
                      >
                        {{
                          vault.vaultId === currentVaultId
                            ? t('actions.deleteWithSync')
                            : t('actions.delete')
                        }}
                      </UButton>
                    </div>
                  </div>
                </template>
              </UAccordion>
            </div>
          </div>
        </template>
      </UCard>
    </div>

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
  </div>
</template>

<script setup lang="ts">
import type { SelectHaexSyncBackends } from '~/database/schemas'

const { t } = useI18n()
const { add } = useToast()

const syncBackendsStore = useSyncBackendsStore()
const syncEngineStore = useSyncEngineStore()
const syncOrchestratorStore = useSyncOrchestratorStore()
const syncConfigStore = useSyncConfigStore()
const vaultStore = useVaultStore()

const { backends: syncBackends } = storeToRefs(syncBackendsStore)
const { currentVaultId, currentVaultName } = storeToRefs(vaultStore)
const { config: syncConfig } = storeToRefs(syncConfigStore)

// Local state
const showAddBackendForm = ref(false)
const isLoading = ref(false)

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

// Server vaults management state - grouped by backend
interface ServerVault {
  vaultId: string
  encryptedVaultName: string
  vaultNameNonce: string
  salt: string
  createdAt: string
  decryptedName?: string
}

interface GroupedServerVaults {
  backend: SelectHaexSyncBackends
  vaults: ServerVault[]
  isLoading: boolean
  error: string | null
}

const groupedServerVaults = ref<GroupedServerVaults[]>([])
const isLoadingAllServerVaults = ref(false)

// Sync configuration
const syncMode = ref(syncConfig.value.mode)
const continuousDebounceMs = ref(syncConfig.value.continuousDebounceMs)
const periodicIntervalMs = ref(syncConfig.value.periodicIntervalMs)

const syncModeOptions = computed(() => [
  {
    value: 'continuous',
    label: t('config.mode.continuous.label'),
  },
  {
    value: 'periodic',
    label: t('config.mode.periodic.label'),
  },
])

const onSyncModeChangeAsync = async (mode: string) => {
  try {
    if (mode !== 'continuous' && mode !== 'periodic') {
      throw new Error(`Invalid sync mode: ${mode}`)
    }
    await syncConfigStore.saveConfigAsync({ mode })
    add({
      color: 'success',
      description: t('config.saveSuccess'),
    })
  } catch (error) {
    console.error('Failed to save sync mode:', error)
    add({
      color: 'error',
      description: t('config.saveError'),
    })
  }
}

const saveContinuousDebounceAsync = async () => {
  try {
    await syncConfigStore.saveConfigAsync({
      continuousDebounceMs: continuousDebounceMs.value,
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
    await syncConfigStore.saveConfigAsync({
      periodicIntervalMs: periodicIntervalMs.value,
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
  isLoading.value = true

  try {
    // 1. First create a temporary backend entry to get an ID for Supabase client initialization
    // We need the backend ID to initialize the client with the correct storage key
    const tempBackend = await syncBackendsStore.addBackendAsync({
      name: new URL(newBackend.serverUrl).host,
      serverUrl: newBackend.serverUrl,
      email: newBackend.email,
      password: newBackend.password,
      enabled: false, // Start disabled until credentials are verified
      vaultId: currentVaultId.value,
    })

    if (!tempBackend) {
      throw new Error('Failed to create backend entry')
    }

    const backendId = tempBackend.id

    try {
      // 2. Initialize Supabase client with the backend ID
      console.log(
        '🔐 Initializing Supabase client and verifying credentials...',
      )
      await syncEngineStore.initSupabaseClientAsync(backendId)

      // 3. Verify credentials by signing in
      if (!syncEngineStore.supabaseClient) {
        throw new Error('Supabase client not initialized')
      }

      const { error: signInError } =
        await syncEngineStore.supabaseClient.auth.signInWithPassword({
          email: newBackend.email,
          password: newBackend.password,
        })

      if (signInError) {
        throw new Error(`Authentication failed: ${signInError.message}`)
      }

      console.log('✅ Credentials verified successfully')

      // 4. Enable the backend now that credentials are verified
      await syncBackendsStore.updateBackendAsync(backendId, {
        enabled: true,
      })

      // 5. Reload backends to ensure they're in the store
      await syncBackendsStore.loadBackendsAsync()
      loadAllServerVaultsAsync()
      // 6. Ensure sync key (client is now authenticated)
      await syncEngineStore.ensureSyncKeyAsync(
        backendId,
        currentVaultId.value!,
        currentVaultName.value,
        newBackend.password,
      )

      // 7. Start sync
      await syncOrchestratorStore.startSyncAsync()

      console.log('✅ Sync started automatically after backend added')

      add({
        title: t('success.backendAdded'),
        color: 'success',
      })

      // Reset form and close
      cancelAddBackend()
    } catch (error) {
      // If authentication fails, delete the backend entry we just created
      console.error('Authentication failed, removing backend entry')
      await syncBackendsStore.deleteBackendAsync(backendId)
      throw error
    }
  } catch (error) {
    console.error('Failed to add backend:', error)
    add({
      title: t('errors.addBackendFailed'),
      description: error instanceof Error ? error.message : 'Unknown error',
      color: 'error',
    })
  } finally {
    isLoading.value = false
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
      const {
        decryptStringAsync,
        deriveKeyFromPasswordAsync,
        base64ToArrayBuffer,
      } = await import('~/utils/crypto/vaultKey')

      for (const vault of vaults) {
        try {
          const salt = base64ToArrayBuffer(vault.salt)
          const derivedKey = await deriveKeyFromPasswordAsync(
            backend.password,
            salt,
          )
          const decryptedName = await decryptStringAsync(
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

  isLoadingAllServerVaults.value = true

  // Initialize grouped vaults structure
  groupedServerVaults.value = syncBackends.value.map((backend) => ({
    backend,
    vaults: [],
    isLoading: true,
    error: null,
  }))

  // Load vaults for each backend in parallel
  await Promise.allSettled(
    groupedServerVaults.value.map(async (group) => {
      try {
        const vaults = await loadVaultsForBackendAsync(group.backend)

        // Keep all vaults including the currently opened one
        group.vaults = vaults
        group.isLoading = false
      } catch (error) {
        group.error = error instanceof Error ? error.message : 'Unknown error'
        group.isLoading = false
      }
    }),
  )

  isLoadingAllServerVaults.value = false
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

    // Delete remote vault from server
    await syncEngineStore.deleteRemoteVaultAsync(backend.id, vaultId)

    // If this is the current vault, also delete the backend connection
    if (isCurrentVault) {
      await syncBackendsStore.deleteBackendAsync(backend.id)

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
    await syncBackendsStore.loadBackendsAsync()

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
</script>

<i18n lang="yaml">
de:
  title: Synchronisation
  description: Verwalte deine Sync-Backends und Account-Einstellungen
  addBackend:
    title: Backend hinzufügen
  backends:
    title: Sync-Backends
    enabled: Aktiviert
    disabled: Deaktiviert
    connected: Verbunden
    syncing: Synchronisiert
  config:
    title: Sync-Konfiguration
    description: Lege fest, wie und wann Änderungen synchronisiert werden
    mode:
      label: Sync-Modus
      continuous:
        label: Kontinuierlich
        description: Änderungen werden sofort nach einer kurzen Verzögerung synchronisiert (empfohlen)
      periodic:
        label: Periodisch
        description: Änderungen werden in festen Zeitintervallen synchronisiert (datensparsam)
    debounce:
      label: Verzögerung
      description: Wartezeit nach der letzten Änderung, bevor synchronisiert wird
    interval:
      label: Sync-Intervall
      description: Zeitabstand zwischen automatischen Synchronisationen
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
    title: Vault-Übersicht
    description: Hier siehst du alle Vaults, die auf den Sync Servern gespeichert sind. Du kannst verwaiste Vaults löschen, auf die du lokal keinen Zugriff mehr hast.
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
en:
  title: Synchronization
  description: Manage your sync backends and account settings
  addBackend:
    title: Add Backend
  backends:
    title: Sync Backends
    enabled: Enabled
    disabled: Disabled
    connected: Connected
    syncing: Syncing
  config:
    title: Sync Configuration
    description: Configure how and when changes are synchronized
    mode:
      label: Sync Mode
      continuous:
        label: Continuous
        description: Changes are synchronized immediately after a short delay (recommended)
      periodic:
        label: Periodic
        description: Changes are synchronized at fixed intervals (data-saving)
    debounce:
      label: Delay
      description: Wait time after the last change before synchronizing
    interval:
      label: Sync Interval
      description: Time between automatic synchronizations
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
</i18n>
