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

    <HaexSystemSettingsSyncBackendsAddBackendCard
      v-if="showAddBackendForm"
      :loading="isLoading"
      :verification-pending="verificationPending"
      :server-options="serverOptions"
      v-model:identity-id="newBackend.identityId"
      v-model:origin-url="newBackend.originUrl"
      v-model:approved-claims="newBackend.approvedClaims"
      v-model:verification-code-parts="verificationCodeParts"
      @submit="onWizardCompleteAsync"
      @cancel="cancelAddBackend"
      @verify="onVerifyCodeAsync"
      @resend="onResendCodeAsync"
    />

    <!-- Sync Backends List -->
    <div>
      <div
        v-if="syncBackends.length"
        class="space-y-3"
      >
        <HaexSystemSettingsSyncBackendsBackendListItem
          v-for="backend in syncBackends"
          :key="backend.id"
          :backend="backend"
          :grouped-vaults="getGroupedVaults(backend.id)"
          :current-vault-id="currentVaultId"
          :is-re-uploading="isReUploading"
          @toggle="toggleBackendAsync(backend.id)"
          @delete-backend="prepareDeleteBackend(backend)"
          @delete-server-vault="(vault) => prepareDeleteServerVault(backend, vault)"
          @re-upload="prepareReUpload(backend)"
        />
      </div>

      <div
        v-else
        class="text-center py-4 text-muted"
      >
        {{ t('backends.noBackends') }}
      </div>
    </div>

    <HaexSystemSettingsSyncBackendsBackendDialogs
      v-model:show-delete-dialog="showDeleteDialog"
      v-model:show-delete-backend-dialog="showDeleteBackendDialog"
      v-model:show-re-upload-dialog="showReUploadDialog"
      v-model:delete-all-server-data="deleteAllServerData"
      :vault-to-delete-space-id="vaultToDeleteSpaceId"
      :vault-to-delete-name="vaultToDeleteName"
      :current-vault-id="currentVaultId"
      :backend-to-delete-completely="backendToDeleteCompletely"
      :re-upload-backend="reUploadBackend"
      :is-re-uploading="isReUploading"
      @confirm-delete-vault="onConfirmDeleteRemoteVaultAsync"
      @confirm-delete-backend="onConfirmDeleteBackendAsync"
      @confirm-re-upload="onConfirmReUploadAsync"
    />
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import type { SelectHaexSyncBackends } from '~/database/schemas'
import type { ServerVault } from '@/composables/useBackendsActions'

defineEmits<{ back: [] }>()

const { t } = useI18n()
const { add } = useToast()

const syncBackendsStore = useSyncBackendsStore()
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

// Server-vaults loading + async actions
const {
  isReUploading,
  getGroupedVaults,
  loadAllServerVaultsAsync,
  toggleBackendAsync,
  deleteBackendCompletelyAsync,
  deleteRemoteVaultAsync,
  reUploadVaultAsync,
} = useBackendsActions()

// Local state
const showAddBackendForm = ref(false)

const newBackend = reactive({
  originUrl: '',
  identityId: '',
  approvedClaims: {} as Record<string, string>,
})

// Verification state
const verificationPending = ref<{
  did: string
  originUrl: string
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
const reUploadBackend = ref<SelectHaexSyncBackends | null>(null)

// Cancel add backend
const cancelAddBackend = () => {
  showAddBackendForm.value = false
  newBackend.originUrl = ''
  newBackend.identityId = ''
  newBackend.approvedClaims = {}
  verificationPending.value = null
  verificationCodeParts.value = []
}

// Handle wizard completion
const onWizardCompleteAsync = async () => {
  const result = await createConnectionAsync({
    originUrl: newBackend.originUrl,
    identityId: newBackend.identityId,
    approvedClaims: newBackend.approvedClaims,
  })

  if (!result) {
    if (connectionError.value) {
      if (connectionError.value.includes('already exists')) {
        add({
          title: t('errors.backendAlreadyExists'),
          description: t('errors.backendAlreadyExistsDescription', {
            originUrl: newBackend.originUrl,
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
      originUrl: result.originUrl,
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

  const { did, originUrl, identityId } = verificationPending.value

  const verified = await verifyEmailAsync(
    originUrl,
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
  const backendId = await completeConnectionAsync({ originUrl, identityId })

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
  const { originUrl, did } = verificationPending.value
  const sent = await resendVerificationAsync(originUrl, did)
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

// Prepare delete backend
const prepareDeleteBackend = (backend: SelectHaexSyncBackends) => {
  backendToDeleteCompletely.value = backend
  showDeleteBackendDialog.value = true
}

// Confirm delete backend
const onConfirmDeleteBackendAsync = async () => {
  const backend = backendToDeleteCompletely.value
  if (!backend) return

  const ok = await deleteBackendCompletelyAsync(backend)
  if (ok) {
    showDeleteBackendDialog.value = false
    backendToDeleteCompletely.value = null
  }
}

// Prepare delete server vault
const prepareDeleteServerVault = (
  backend: SelectHaexSyncBackends,
  vault: ServerVault,
) => {
  backendToDelete.value = backend
  vaultToDeleteSpaceId.value = vault.spaceId
  vaultToDeleteName.value =
    vault.decryptedName || t('vaultOverview.encryptedName')
  deleteAllServerData.value = false
  showDeleteDialog.value = true
}

// Confirm delete remote vault
const onConfirmDeleteRemoteVaultAsync = async () => {
  const backend = backendToDelete.value
  const spaceId = vaultToDeleteSpaceId.value
  if (!backend || !spaceId) return

  const ok = await deleteRemoteVaultAsync({
    backend,
    spaceId,
    deleteAll: deleteAllServerData.value,
  })
  if (ok) {
    // Close dialog and reset state
    showDeleteDialog.value = false
    backendToDelete.value = null
    vaultToDeleteSpaceId.value = null
    vaultToDeleteName.value = null
    deleteAllServerData.value = false
  }
}

// Prepare re-upload for a specific backend
const prepareReUpload = (backend: SelectHaexSyncBackends) => {
  reUploadBackend.value = backend
  showReUploadDialog.value = true
}

// Confirm re-upload
const onConfirmReUploadAsync = async () => {
  const backend = reUploadBackend.value
  if (!backend) return

  const ok = await reUploadVaultAsync(backend)
  if (ok) {
    showReUploadDialog.value = false
    reUploadBackend.value = null
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
    originUrlUpdated: Server-URL aktualisiert
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
    backendAlreadyExistsDescription: Es besteht bereits eine Verbindung zu {originUrl}
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
    originUrlUpdated: Server URL updated
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
    backendAlreadyExistsDescription: A connection to {originUrl} already exists
</i18n>
