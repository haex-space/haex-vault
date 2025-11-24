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
        size="xl"
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
import { eq } from 'drizzle-orm'
import { schema } from '~/database'

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
  password: string
  newVaultPassword?: string
}) => {
  isLoading.value = true

  try {
    // 1. Validate required password
    if (!wizardData.newVaultPassword) {
      throw new Error('Vault password is required')
    }

    // 2. Create and open new local vault
    console.log('📦 Creating new vault:', wizardData.localVaultName)
    const localVaultId = await vaultStore.createAsync({
      vaultName: wizardData.localVaultName,
      password: wizardData.newVaultPassword,
    })

    if (!localVaultId) {
      throw new Error('Failed to create vault')
    }

    console.log('✅ Vault created with local ID:', localVaultId)

    // IMPORTANT: Override local vault_id with remote vault_id to ensure sync works
    console.log(`🔄 Setting vault_id to remote ID: ${wizardData.vaultId}`)
    await vaultStore.currentVault?.drizzle
      .update(schema.haexVaultSettings)
      .set({ value: wizardData.vaultId })
      .where(eq(schema.haexVaultSettings.key, 'vault_id'))

    // Update the openVaults map: move vault from old ID to new ID
    const vaultData = vaultStore.openVaults[localVaultId]
    if (vaultData) {
      vaultStore.openVaults = {
        ...vaultStore.openVaults,
        [wizardData.vaultId]: vaultData,
      }
      // Remove old key
      const { [localVaultId]: _, ...rest } = vaultStore.openVaults
      vaultStore.openVaults = rest
    }

    console.log('✅ Vault ID updated to remote ID:', wizardData.vaultId)

    // Close drawer
    open.value = false

    // Navigate to vault with remote vault ID
    await navigateTo(
      useLocaleRoute()({
        name: 'desktop',
        params: { vaultId: wizardData.vaultId },
        query: { remoteSync: 'true' },
      }),
    )

    // 3. Now that vault is open and currentVaultId is set, configure backend
    const existingBackend = await syncBackendsStore.findBackendByCredentialsAsync(
      wizardData.serverUrl,
      wizardData.email,
    )

    let backendId: string

    if (existingBackend) {
      // Backend exists, update password and vaultId if changed
      console.log('✅ Backend already exists, updating password and vaultId')
      await syncBackendsStore.updateBackendAsync(existingBackend.id, {
        password: wizardData.password,
        vaultId: wizardData.vaultId,
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
        vaultId: wizardData.vaultId,
        email: wizardData.email,
        password: wizardData.password,
        enabled: true,
      })
    }

    // 4. Ensure sync key exists (use the backend vaultId from wizard)
    // Use backend password for encrypting vault key on server
    await syncEngineStore.ensureSyncKeyAsync(
      backendId,
      wizardData.vaultId,
      wizardData.vaultName,
      wizardData.password, // Backend password for server encryption
    )

    // 5. Start sync (Supabase client already initialized in wizard)
    await syncOrchestratorStore.startSyncAsync()

    console.log('✅ Vault created and sync started')

    add({
      title: t('success.title'),
      description: t('success.description'),
      color: 'success',
    })
  } catch (error) {
    console.error('Failed to connect backend and create vault:', error)
    add({
      title: t('error.title'),
      description: error instanceof Error ? error.message : 'Unknown error',
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
  title: Mit Backend verbinden
  description: Verbinde dich mit einem Sync-Backend und erstelle eine neue Vault
  button:
    label: Mit Backend verbinden
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

en:
  title: Connect to Backend
  description: Connect to a sync backend and create a new vault
  button:
    label: Connect to Backend
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
</i18n>
