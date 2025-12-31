<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
    :description="t('description')"
  >
    <!-- Trigger Button -->
    <template #trigger>
      <UiButton
        :label="t('button.label')"
        :ui="{
          base: 'px-4 py-3',
        }"
        icon="i-lucide-cloud-download"
        size="lg"
        variant="outline"
        block
      />
    </template>

    <!-- Content -->
    <template #content>
      <HaexSyncConnectWizard
        ref="wizardRef"
        :is-loading="isLoading"
        @complete="onWizardCompleteAsync"
        @cancel="open = false"
      />
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
const { t } = useI18n({ useScope: 'local' })
const { add } = useToast()

const open = defineModel<boolean>('open', { default: false })

const syncBackendsStore = useSyncBackendsStore()
const syncEngineStore = useSyncEngineStore()
const syncOrchestratorStore = useSyncOrchestratorStore()
const vaultStore = useVaultStore()

const wizardRef = ref()
const isLoading = ref(false)

// Handle wizard completion
const onWizardCompleteAsync = async (wizardData: {
  backendId: string
  vaultId: string
  vaultName: string
  localVaultName: string
  serverUrl: string
  email: string
  serverPassword: string
  vaultPassword: string
}) => {
  isLoading.value = true

  let localVaultId: string | null = null

  try {
    // 1. Validate required password
    if (!wizardData.vaultPassword) {
      throw new Error('Vault password is required')
    }

    // 2. Set up temporary backend FIRST (for vault key fetch)
    console.log('📤 Setting up temporary backend for initial sync')
    syncBackendsStore.setTemporaryBackend({
      id: wizardData.backendId,
      name: new URL(wizardData.serverUrl).host,
      serverUrl: wizardData.serverUrl,
      vaultId: wizardData.vaultId,
      email: wizardData.email,
      password: wizardData.serverPassword, // Server login password
      enabled: true,
    })

    // 3. Verify vault password by fetching and decrypting the vault key BEFORE creating local vault
    // This prevents creating orphan vault files if the password is wrong
    console.log('🔐 Verifying vault password...')
    await syncEngineStore.ensureSyncKeyAsync(
      wizardData.backendId,
      wizardData.vaultId,
      wizardData.vaultName,
      wizardData.vaultPassword, // Vault encryption password
      wizardData.serverUrl, // Initial sync: fetch from server directly
    )
    console.log('✅ Vault password verified')

    // 4. Now create minimal vault with remote vault_id (DB + vault_id only)
    // No workspaces, devices, or backends are created yet
    console.log('📦 Creating minimal vault:', wizardData.localVaultName)
    console.log('📦 Using remote vault_id:', wizardData.vaultId)

    localVaultId = await vaultStore.createAsync({
      vaultName: wizardData.localVaultName,
      password: wizardData.vaultPassword,
      vaultId: wizardData.vaultId, // Pass remote vault_id directly
    })

    if (!localVaultId) {
      throw new Error('Failed to create vault')
    }

    console.log('✅ Vault created with ID:', localVaultId)

    // Close drawer before navigating
    open.value = false

    // 5. Navigate to vault
    // The vault.vue page will detect remoteSync=true and wait for initial sync
    await navigateTo(
      useLocaleRoute()({
        name: 'desktop',
        params: { vaultId: wizardData.vaultId },
        query: { remoteSync: 'true' },
      }),
    )

    // 6. Perform initial pull using temporary backend
    // This pulls ALL data from server before creating any local data
    // After successful pull, the backend is persisted to DB
    console.log('🔄 Starting initial pull from remote vault')
    await syncOrchestratorStore.performInitialPullAsync()

    // 7. Start normal sync (backend is now in DB from step 6)
    console.log('🔄 Starting normal sync')
    await syncOrchestratorStore.startSyncAsync()

    console.log('✅ Vault created and sync started')

    add({
      title: t('success.title'),
      description: t('success.description'),
      color: 'success',
    })
  } catch (error) {
    console.error('Failed to connect backend and create vault:', error)

    // Clean up: delete the vault file if it was created but a later step failed
    if (localVaultId) {
      console.log('🗑️ Cleaning up partially created vault...')
      try {
        await vaultStore.deleteAsync(wizardData.localVaultName)
        console.log('✅ Partial vault cleaned up')
      } catch (cleanupError) {
        console.warn('⚠️ Failed to clean up partial vault:', cleanupError)
      }
    }

    // Clear temporary backend on error
    syncBackendsStore.clearTemporaryBackend()

    // Check if it's a network error and provide user-friendly message
    const isNetworkError = error instanceof Error && error.message.startsWith('NETWORK_ERROR:')
    const errorMessage = isNetworkError
      ? t('error.networkError')
      : (error instanceof Error ? error.message : 'Unknown error')

    add({
      title: isNetworkError ? t('error.networkTitle') : t('error.title'),
      description: errorMessage,
      color: 'error',
    })
  } finally {
    isLoading.value = false
  }
}

// Watch for drawer close to reset wizard
watch(open, (isOpen) => {
  if (!isOpen) {
    wizardRef.value?.clearForm()
  }
})
</script>

<i18n lang="yaml">
de:
  title: Vault verbinden
  description: Verbinde dich mit einem Sync-Backend und erstelle eine neue Vault
  button:
    label: Vault verbinden
  divider: Vault-Einstellungen
  vaultName:
    label: Vaultname
  vaultPassword:
    label: Vault-Passwort
  vaultPasswordConfirm:
    label: Vault-Passwort bestätigen
  create: Verbinden und erstellen
  cancel: Abbrechen
  success:
    loginTitle: Backend-Login erfolgreich
    loginDescription: Anmeldung am Backend war erfolgreich
    title: Vault erstellt
    description: Vault wurde erstellt und mit Backend synchronisiert
  error:
    title: Verbindung fehlgeschlagen
    networkTitle: Keine Internetverbindung
    networkError: Der Sync-Server konnte nicht erreicht werden. Bitte überprüfe deine Internetverbindung und versuche es erneut.

en:
  title: Connect Vault
  description: Connect to a sync backend and create a new vault
  button:
    label: Connect Vault
  divider: Vault Settings
  vaultName:
    label: Vault Name
  vaultPassword:
    label: Vault Password
  vaultPasswordConfirm:
    label: Confirm Vault Password
  create: Connect and Create
  cancel: Cancel
  success:
    loginTitle: Backend login successful
    loginDescription: Successfully signed in to backend
    title: Vault created
    description: Vault created and synced with backend
  error:
    title: Connection Failed
    networkTitle: No Internet Connection
    networkError: Unable to reach the sync server. Please check your internet connection and try again.
</i18n>
