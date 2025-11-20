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

    <div class="p-6 space-y-6">
      <!-- Add Backend Section -->
      <div>
        <div class="flex items-center justify-between mb-4">
          <h3 class="font-semibold">
            {{ t('backends.title') }}
          </h3>
          <UButton
            v-if="!showAddBackendForm"
            color="primary"
            @click="showAddBackendForm = true"
          >
            {{ t('actions.addBackend') }}
          </UButton>
        </div>

        <!-- Add Backend Wizard -->
        <div
          v-if="showAddBackendForm"
          class="card bg-base-200 p-4 mb-4"
        >
          <HaexSyncBackendConnectWizard
            ref="wizardRef"
            :is-loading="isLoading"
            show-cancel
            @complete="onWizardCompleteAsync"
            @cancel="cancelAddBackend"
          />
        </div>
      </div>

      <!-- Sync Backends List -->
      <div v-if="syncBackends.length > 0">
        <div class="space-y-2">
          <div
            v-for="backend in syncBackends"
            :key="backend.id"
            class="card bg-base-200 p-4"
          >
            <div class="flex items-center justify-between">
              <div>
                <p class="font-medium">{{ backend.name }}</p>
                <p class="text-sm text-base-content/60">
                  {{ backend.serverUrl }}
                </p>
                <div class="flex gap-2 mt-2">
                  <span
                    class="badge badge-sm"
                    :class="backend.enabled ? 'badge-success' : 'badge-ghost'"
                  >
                    {{
                      backend.enabled
                        ? t('backends.enabled')
                        : t('backends.disabled')
                    }}
                  </span>
                  <span
                    v-if="getSyncState(backend.id)?.isConnected"
                    class="badge badge-sm badge-info"
                  >
                    {{ t('backends.connected') }}
                  </span>
                  <span
                    v-else-if="getSyncState(backend.id)?.isSyncing"
                    class="badge badge-sm badge-warning"
                  >
                    {{ t('backends.syncing') }}
                  </span>
                </div>
              </div>
              <div class="flex gap-2">
                <UButton
                  size="sm"
                  :color="backend.enabled ? 'neutral' : 'primary'"
                  @click="toggleBackendAsync(backend.id)"
                >
                  {{
                    backend.enabled ? t('actions.disable') : t('actions.enable')
                  }}
                </UButton>
              </div>
            </div>
          </div>
        </div>
      </div>

    </div>
  </div>
</template>

<script setup lang="ts">
const { t } = useI18n()
const { add } = useToast()

const syncBackendsStore = useSyncBackendsStore()
const syncEngineStore = useSyncEngineStore()
const syncOrchestratorStore = useSyncOrchestratorStore()

const { backends: syncBackends } = storeToRefs(syncBackendsStore)

// Local state
const showAddBackendForm = ref(false)
const isLoading = ref(false)
const wizardRef = ref()

// Cancel add backend
const cancelAddBackend = () => {
  showAddBackendForm.value = false
  wizardRef.value?.clearForm()
}

// Handle wizard completion
const onWizardCompleteAsync = async (wizardData: {
  backendId: string
  vaultId: string
  vaultName: string
  serverUrl: string
  email: string
  password: string
}) => {
  isLoading.value = true

  try {
    // 1. Check if backend with these credentials already exists
    const existingBackend = await syncBackendsStore.findBackendByCredentialsAsync(
      wizardData.serverUrl,
      wizardData.email,
    )

    let backendId: string

    if (existingBackend) {
      // Backend exists, update password if changed
      console.log('✅ Backend already exists, updating password')
      await syncBackendsStore.updateBackendAsync(existingBackend.id, {
        password: wizardData.password,
      })
      backendId = existingBackend.id
    } else {
      // Create new backend with credentials
      console.log('📤 Creating new backend with credentials')
      backendId = wizardData.backendId
      await syncBackendsStore.addBackendAsync({
        id: backendId,
        name: new URL(wizardData.serverUrl).host,
        serverUrl: wizardData.serverUrl,
        email: wizardData.email,
        password: wizardData.password,
        enabled: true,
      })
    }

    // 2. Reload backends to ensure they're in the store
    await syncBackendsStore.loadBackendsAsync()

    // 3. Initialize Supabase client and ensure sync key
    await syncEngineStore.ensureSyncKeyAsync(
      backendId,
      wizardData.vaultId,
      wizardData.vaultName,
      wizardData.password,
    )

    // 4. Initialize Supabase client and start sync
    await syncEngineStore.initSupabaseClientAsync(backendId)
    await syncOrchestratorStore.startSyncAsync()

    console.log('✅ Sync started automatically after backend added')

    add({
      title: t('success.backendAdded'),
      color: 'success',
    })

    // Reset form and close
    cancelAddBackend()
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
  actions:
    add: Hinzufügen
    cancel: Abbrechen
    addBackend: Backend hinzufügen
    enable: Aktivieren
    disable: Deaktivieren
  success:
    signedIn: Erfolgreich angemeldet
    signedOut: Erfolgreich abgemeldet
    serverUrlUpdated: Server-URL aktualisiert
    backendAdded: Backend hinzugefügt
    backendEnabled: Backend aktiviert
    backendDisabled: Backend deaktiviert
    syncStarted: Synchronisation gestartet
    syncStopped: Synchronisation gestoppt
  errors:
    noBackend: Kein Backend konfiguriert
    noServerUrl: Bitte trage zuerst die Server-URL ein
    initFailed: Initialisierung fehlgeschlagen
    signInFailed: Anmeldung fehlgeschlagen
    signOutFailed: Abmeldung fehlgeschlagen
    addBackendFailed: Backend konnte nicht hinzugefügt werden
    toggleFailed: Status konnte nicht geändert werden
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
  actions:
    add: Add
    cancel: Cancel
    addBackend: Add Backend
    enable: Enable
    disable: Disable
  success:
    signedIn: Successfully signed in
    signedOut: Successfully signed out
    serverUrlUpdated: Server URL updated
    backendAdded: Backend added
    backendEnabled: Backend enabled
    backendDisabled: Backend disabled
    syncStarted: Sync started
    syncStopped: Sync stopped
  errors:
    noBackend: No backend configured
    noServerUrl: Please enter the server URL first
    initFailed: Initialization failed
    signInFailed: Sign in failed
    signOutFailed: Sign out failed
    addBackendFailed: Failed to add backend
    toggleFailed: Failed to toggle status
</i18n>
