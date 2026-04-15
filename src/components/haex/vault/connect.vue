<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
    :description="t('description')"
    :dismissible="isDismissible"
  >
    <!-- Trigger Button -->
    <template #trigger>
      <UiButton
        :label="t('button.label')"
        :ui="{
          base: 'px-4 py-3',
        }"
        icon="i-lucide-cloud-download"
        variant="outline"
        block
      />
    </template>

    <!-- Content -->
    <template #body>
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
const lastVaultStore = useLastVaultStore()

const wizardRef = ref()
const isLoading = ref(false)

// Prevent accidental close during login steps (email + OTP)
const isDismissible = computed(
  () => (wizardRef.value?.currentStepIndex?.value ?? 0) > 1,
)

// Handle wizard completion
const onWizardCompleteAsync = async (wizardData: {
  backendId: string
  spaceId: string
  vaultName: string
  localVaultName: string
  serverUrl: string
  identityId: string
  identityPublicKey: string
  identityPrivateKey: string
  identityDid: string
  vaultPassword: string
  isNewVault: boolean
}) => {
  isLoading.value = true

  let localVaultId: string | null = null

  try {
    // 1. Validate required password
    if (!wizardData.vaultPassword) {
      throw new Error('Vault password is required')
    }

    // 2. Ensure identity is available in-memory (vault DB may not be open yet)
    const identityStore = useIdentityStore()
    let identityId = wizardData.identityId
    if (!identityId) {
      identityId = crypto.randomUUID()
      identityStore.registerTemporaryIdentity({
        id: identityId,
        privateKey: wizardData.identityPrivateKey,
        did: wizardData.identityDid,
        name: 'Recovered Identity',
      })
    }

    // 3. Set up temporary backend (for vault key fetch/upload)
    syncBackendsStore.setTemporaryBackend({
      id: wizardData.backendId,
      name: new URL(wizardData.serverUrl).host,
      homeServerUrl: wizardData.serverUrl,
      spaceId: wizardData.spaceId,
      identityId,
      enabled: true,
    })

    if (wizardData.isNewVault) {
      // NEW VAULT: Generate spaceId and upload vault key
      // POST /sync/vault-key creates the space (type: vault) + partition via DB trigger
      wizardData.spaceId = crypto.randomUUID()

      const { generateNewVaultKey, uploadVaultKeyAsync, cacheSyncKey } = await import('@/stores/sync/engine/vaultKey')

      const vaultKey = generateNewVaultKey()

      await uploadVaultKeyAsync(
        wizardData.serverUrl,
        wizardData.spaceId,
        vaultKey,
        wizardData.vaultName,
        wizardData.vaultPassword,
        wizardData.identityPublicKey,
        wizardData.identityPrivateKey,
        wizardData.identityDid,
      )

      // Cache the vault key for immediate use
      cacheSyncKey(wizardData.spaceId, vaultKey)
    } else {
      // EXISTING VAULT: Verify password by fetching and decrypting the vault key
      // This prevents creating orphan vault files if the password is wrong
      await syncEngineStore.ensureSyncKeyAsync(
        wizardData.backendId,
        wizardData.spaceId,
        wizardData.vaultName,
        wizardData.vaultPassword,
        wizardData.serverUrl,
      )
    }

    // 4. Now create minimal vault with space_id (DB + space_id only)
    // No workspaces, devices, or backends are created yet
    localVaultId = await vaultStore.createAsync({
      vaultName: wizardData.localVaultName,
      password: wizardData.vaultPassword,
      spaceId: wizardData.spaceId,
    })

    if (!localVaultId) {
      throw new Error('Failed to create vault')
    }

    // Close drawer before navigating
    open.value = false

    // 5. Navigate to vault (sets currentVaultId via route params)
    // The vault.vue page will detect remoteSync=true and wait for initial sync
    await navigateTo(
      useLocaleRoute()({
        name: 'desktop',
        params: { vaultId: wizardData.spaceId },
        query: { remoteSync: 'true' },
      }),
    )
    // 6. Persist recovered identity to DB (vault is now open via route)
    await identityStore.importIdentityAsync({
      privateKey: wizardData.identityPrivateKey,
      did: wizardData.identityDid,
      name: 'Recovered Identity',
    })

    // Update temporary backend with the DB-persisted identity ID
    const persistedIdentity = await identityStore.getIdentityByDidAsync(wizardData.identityDid)
    if (persistedIdentity) {
      syncBackendsStore.setTemporaryBackend({
        ...syncBackendsStore.temporaryBackend!,
        identityId: persistedIdentity.id,
      })
    }

    // 7. Perform initial pull using temporary backend
    // For new vaults: this will be empty but sets up the sync infrastructure
    // For existing vaults: this pulls ALL data from server
    // After successful pull, the backend is persisted to DB
    // NOTE: performInitialPullAsync now also reloads stores (extensions, workspaces, desktop items)
    // before signaling sync complete - this prevents race conditions with vault.vue
    await syncOrchestratorStore.performInitialPullAsync()

    // 7. Start normal sync (backend is now in DB from step 6)
    await syncOrchestratorStore.startSyncAsync()

    add({
      title: wizardData.isNewVault ? t('success.titleNew') : t('success.title'),
      description: wizardData.isNewVault ? t('success.descriptionNew') : t('success.description'),
      color: 'success',
    })
  } catch (error) {
    console.error('Failed to connect backend and create vault:', error)

    // Clean up: delete the vault file if it was created but a later step failed
    if (localVaultId) {
      try {
        await lastVaultStore.removeVaultAsync(wizardData.localVaultName)
      } catch (cleanupError) {
        console.warn('Failed to clean up partial vault:', cleanupError)
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

// Watch for drawer close — only reset if past the login steps (email + OTP)
watch(open, (isOpen) => {
  if (!isOpen && (wizardRef.value?.currentStepIndex ?? 0) > 1) {
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
    title: Vault verbunden
    description: Vault wurde verbunden und mit Backend synchronisiert
    titleNew: Vault erstellt
    descriptionNew: Neuer Vault wurde erstellt und mit Backend synchronisiert
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
    title: Vault connected
    description: Vault connected and synced with backend
    titleNew: Vault created
    descriptionNew: New vault created and synced with backend
  error:
    title: Connection Failed
    networkTitle: No Internet Connection
    networkError: Unable to reach the sync server. Please check your internet connection and try again.
</i18n>
