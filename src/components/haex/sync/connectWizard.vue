<template>
  <div class="space-y-6">
    <!-- Stepper -->
    <UStepper
      v-model="wizard.currentStepIndex.value"
      :items="steps"
      :linear="false"
    >
      <template #loginEmail>
        <HaexSyncConnectWizardStepLoginEmail @otp-requested="onOtpRequested" />
      </template>

      <template #loginOtp>
        <HaexSyncConnectWizardStepLoginOtp
          :origin-url="wizard.otpServerUrl.value"
          :email="wizard.otpEmail.value"
          @recovered="onRecoveryComplete"
          @change-email="wizard.currentStepIndex.value = 0"
        />
      </template>

      <template #didPassword>
        <HaexSyncConnectWizardStepDidPassword
          v-model:password="wizard.didPassword.value"
          :error="wizard.didPasswordError.value"
        />
      </template>

      <template #selectVault>
        <HaexSyncConnectWizardStepSelectVault
          v-model:local-vault-name="wizard.localVaultName.value"
          v-model:vault-password="wizard.vaultPassword.value"
          v-model:vault-password-confirm="wizard.vaultPasswordConfirm.value"
          :available-vaults="wizard.availableVaults.value"
          :selected-vault-id="wizard.selectedVaultId.value"
          :is-loading-vaults="wizard.isLoadingVaults.value"
          :step3-error="wizard.step3Error.value"
          :is-creating-new-vault="wizard.isCreatingNewVault.value"
          :decrypted-vault-names="wizard.decryptedVaultNames.value"
          :needs-vault-password="wizard.needsVaultPassword.value"
          :is-checking-vault-password="wizard.isCheckingVaultPassword.value"
          :vault-password-verified="wizard.vaultPasswordVerified.value"
          :vault-name-exists="wizard.vaultNameExists.value"
          :step3-errors="wizard.step3Errors"
          :check="wizard.check.value"
          :wizard-schema="wizardSchema"
          @select-vault="selectVault"
          @select-new-vault="selectNewVault"
          @check-vault-name="checkVaultNameExistsAsync"
        />
      </template>
    </UStepper>

    <!-- Actions -->
    <div class="flex gap-3 mt-6">
      <UButton
        color="neutral"
        variant="outline"
        @click="cancel"
      >
        {{ t('actions.cancel') }}
      </UButton>
      <UButton
        v-if="wizard.currentStepIndex.value > 0"
        color="neutral"
        variant="outline"
        @click="wizard.previousStep"
      >
        {{ t('actions.back') }}
      </UButton>
      <div class="flex-1" />
      <UButton
        v-if="wizard.currentStepIndex.value < 3"
        color="primary"
        :disabled="!wizard.canProceed.value"
        :loading="wizard.isLoading.value"
        @click="nextStep"
      >
        {{ t('actions.next') }}
      </UButton>
      <UButton
        v-else
        color="primary"
        :disabled="!wizard.canComplete.value || wizard.isCheckingVaultPassword.value"
        :loading="wizard.isLoading.value || wizard.isCheckingVaultPassword.value"
        @click="completeSetupAsync"
      >
        {{ wizard.vaultPasswordVerified.value ? t('actions.open') : t('actions.complete') }}
      </UButton>
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  decryptPrivateKeyAsync,
  decryptVaultKey,
} from '@haex-space/vault-sdk'
import { decryptVaultNameAsync } from '@/utils/crypto/vaultName'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import type { StepperItem } from '@nuxt/ui'
import { createConnectWizardSchema } from './connectWizardSchema'
import type { RecoveryKeyData } from '~/composables/useIdentityRecovery'

const { t } = useI18n()
const { add } = useToast()
const { decryptAndVerifyAsync } = useIdentityRecovery()

// Create validation schema with i18n
const wizardSchema = computed(() => createConnectWizardSchema(t))

defineProps<{
  showCancel?: boolean
}>()

const emit = defineEmits<{
  complete: [
    {
      backendId: string
      spaceId: string
      vaultName: string
      localVaultName: string
      originUrl: string
      identityId: string
      identityPublicKey: string
      identityPrivateKey: string
      identityDid: string
      vaultPassword: string
      isNewVault: boolean
    },
  ]
  cancel: []
}>()

const wizard = useConnectWizard()

// Keyboard shortcuts with VueUse
const keys = useMagicKeys()
const escape = computed(() => keys.escape?.value ?? false)
const enter = computed(() => keys.enter?.value ?? false)

const steps = computed(
  () =>
    [
      {
        slot: 'loginEmail' as const,
        label: t('steps.loginEmail.title'),
        icon: 'i-lucide-mail',
      },
      {
        slot: 'loginOtp' as const,
        label: t('steps.loginOtp.title'),
        icon: 'i-lucide-shield-check',
      },
      {
        slot: 'didPassword' as const,
        label: t('steps.didPassword.title'),
        icon: 'i-lucide-fingerprint',
      },
      {
        slot: 'selectVault' as const,
        label: t('steps.selectVault.title'),
        icon: 'i-lucide-folder',
      },
    ] satisfies StepperItem[],
)

const { currentVaultPassword } = storeToRefs(useVaultStore())

// Keyboard shortcuts handlers
whenever(escape, () => {
  cancel()
})

whenever(enter, () => {
  if (wizard.currentStepIndex.value < 3 && wizard.canProceed.value && !wizard.isLoading.value) {
    nextStep()
  } else if (
    wizard.currentStepIndex.value === 3 &&
    wizard.canComplete.value &&
    !wizard.isLoading.value
  ) {
    completeSetupAsync()
  }
})

// Methods
const onOtpRequested = (data: { originUrl: string; email: string }) => {
  wizard.otpServerUrl.value = data.originUrl
  wizard.otpEmail.value = data.email
  wizard.currentStepIndex.value = 1
}

const nextStep = async () => {
  // Step 2: DID password → decrypt private key, load & decrypt vault names
  if (wizard.currentStepIndex.value === 2) {
    if (!wizard.recoveredKeyData.value) return
    wizard.isLoading.value = true
    wizard.didPasswordError.value = ''

    try {
      const valid = await decryptAndVerifyAsync(
        wizard.recoveredKeyData.value,
        wizard.didPassword.value,
      )
      if (!valid) {
        wizard.didPasswordError.value = t('errors.wrongDidPassword')
        return
      }

      // Decrypt private key and store for vault name decryption
      wizard.decryptedPrivateKey.value = await decryptPrivateKeyAsync(
        wizard.recoveredKeyData.value.encryptedPrivateKey,
        wizard.recoveredKeyData.value.privateKeyNonce,
        wizard.recoveredKeyData.value.privateKeySalt,
        wizard.didPassword.value,
      )

      // Load vaults and decrypt names
      await loadVaultsAsync()
      await decryptVaultNamesAsync(wizard.decryptedPrivateKey.value)

      wizard.currentStepIndex.value++
    } catch {
      wizard.didPasswordError.value = t('errors.wrongDidPassword')
    } finally {
      wizard.isLoading.value = false
    }
  }
}

const loadVaultsAsync = async () => {
  if (!wizard.decryptedPrivateKey.value || !wizard.recoveredKeyData.value) return

  wizard.isLoadingVaults.value = true

  try {
    const response = await fetchWithDidAuth(
      `${wizard.credentials.value.originUrl}/sync/vaults`,
      wizard.decryptedPrivateKey.value,
      wizard.recoveredKeyData.value.did,
    )

    if (!response.ok) {
      throw new Error('Failed to fetch vaults')
    }

    const data = await response.json()
    wizard.availableVaults.value = data.vaults
  } catch (error) {
    console.error('Failed to load vaults:', error)
    add({
      title: t('errors.loadVaultsFailed'),
      description: error instanceof Error ? error.message : 'Unknown error',
      color: 'error',
    })
  } finally {
    wizard.isLoadingVaults.value = false
  }
}

const decryptVaultNamesAsync = async (privateKeyBase64: string) => {
  const names: Record<string, string> = {}
  for (const vault of wizard.availableVaults.value) {
    try {
      names[vault.spaceId] = await decryptVaultNameAsync(
        vault.encryptedVaultName,
        vault.vaultNameNonce,
        vault.vaultNameSalt,
        vault.ephemeralPublicKey,
        privateKeyBase64,
      )
    } catch {
      // Decryption failed — keep showing fallback
    }
  }
  wizard.decryptedVaultNames.value = names
}

const checkVaultNameExistsAsync = async () => {
  if (!wizard.localVaultName.value) {
    wizard.vaultNameExists.value = false
    return
  }

  try {
    const vaultStore = useVaultStore()
    const exists = await vaultStore.vaultExistsAsync(wizard.localVaultName.value)
    wizard.vaultNameExists.value = exists
  } catch (error) {
    console.error('Failed to check vault name:', error)
    wizard.vaultNameExists.value = false
  }
}

const completeSetupAsync = async () => {
  wizard.check.value = true
  await nextTick()

  if (!wizard.canComplete.value) return
  if (!wizard.isCreatingNewVault.value && !wizard.selectedVaultId.value) return

  // Determine effective vault password
  const effectivePassword = wizard.needsVaultPassword.value || wizard.isCreatingNewVault.value
    ? wizard.vaultPassword.value
    : wizard.didPassword.value

  const backendId = crypto.randomUUID()

  if (wizard.isCreatingNewVault.value) {
    emit('complete', {
      backendId,
      spaceId: '', // Server generates via /partitions/create
      vaultName: wizard.localVaultName.value,
      localVaultName: wizard.localVaultName.value,
      originUrl: wizard.credentials.value.originUrl,
      identityId: wizard.credentials.value.identityId,
      identityPublicKey: wizard.recoveredKeyData.value!.publicKey,
      identityPrivateKey: wizard.decryptedPrivateKey.value!,
      identityDid: wizard.recoveredKeyData.value!.did,
      vaultPassword: effectivePassword,
      isNewVault: true,
    })
  } else {
    const selectedVault = wizard.availableVaults.value.find(
      (v) => v.spaceId === wizard.selectedVaultId.value,
    )
    if (!selectedVault) return

    emit('complete', {
      backendId,
      spaceId: selectedVault.spaceId,
      vaultName: wizard.localVaultName.value,
      localVaultName: wizard.localVaultName.value,
      originUrl: wizard.credentials.value.originUrl,
      identityId: wizard.credentials.value.identityId,
      identityPublicKey: wizard.recoveredKeyData.value!.publicKey,
      identityPrivateKey: wizard.decryptedPrivateKey.value!,
      identityDid: wizard.recoveredKeyData.value!.did,
      vaultPassword: effectivePassword,
      isNewVault: false,
    })
  }
}

const selectVault = async (spaceId: string) => {
  wizard.selectedVaultId.value = spaceId
  wizard.isCreatingNewVault.value = false
  wizard.needsVaultPassword.value = false
  wizard.vaultPasswordVerified.value = false
  wizard.vaultPassword.value = ''
  wizard.step3Error.value = ''

  // Auto-fill local vault name with decrypted name
  wizard.localVaultName.value = wizard.decryptedVaultNames.value[spaceId] || 'HaexVault'
  checkVaultNameExistsAsync()

  // Try DID password as vault password in background
  await tryDIDPasswordAsVaultPasswordAsync(spaceId)
}

const tryDIDPasswordAsVaultPasswordAsync = async (spaceId: string) => {
  if (!wizard.decryptedPrivateKey.value || !wizard.recoveredKeyData.value) return

  wizard.isCheckingVaultPassword.value = true

  try {
    const response = await fetchWithDidAuth(
      `${wizard.credentials.value.originUrl}/sync/vault-key/${spaceId}`,
      wizard.decryptedPrivateKey.value,
      wizard.recoveredKeyData.value.did,
    )

    if (!response.ok) return

    const data = await response.json()

    // Try decrypting vault key with DID password
    await decryptVaultKey(
      data.vaultKey.encryptedVaultKey,
      data.vaultKey.vaultKeySalt,
      data.vaultKey.vaultKeyNonce,
      wizard.didPassword.value,
    )

    // Success — DID password works as vault password
    wizard.vaultPasswordVerified.value = true
  } catch (error) {
    // OperationError = wrong password → show vault password field
    if (error instanceof Error && error.name === 'OperationError') {
      wizard.needsVaultPassword.value = true
    }
  } finally {
    wizard.isCheckingVaultPassword.value = false
  }
}

const selectNewVault = () => {
  wizard.isCreatingNewVault.value = true
  wizard.selectedVaultId.value = null
  wizard.needsVaultPassword.value = false
  wizard.vaultPassword.value = ''
  wizard.vaultPasswordConfirm.value = ''
  wizard.step3Error.value = ''
  wizard.localVaultName.value = 'HaexVault'
  checkVaultNameExistsAsync()
}

const cancel = () => {
  emit('cancel')
}

const onRecoveryComplete = async (data: {
  originUrl: string
  recoveryKeyData: RecoveryKeyData
  session: {
    access_token: string
    refresh_token: string
    expires_in: number
    expires_at: number
  }
  identity: { publicKey: string; did: string; tier: string }
}) => {
  wizard.isLoading.value = true

  try {
    wizard.credentials.value.originUrl = data.originUrl
    // Look up identity UUID by publicKey (server response only has publicKey)
    const identityStore = useIdentityStore()
    const resolvedIdentity = await identityStore.getIdentityByPublicKeyAsync(data.identity.publicKey)
    wizard.credentials.value.identityId = resolvedIdentity?.id ?? ''
    wizard.recoveredKeyData.value = data.recoveryKeyData

    // Pre-fill DID password with current vault password if available
    if (currentVaultPassword.value) {
      wizard.didPassword.value = currentVaultPassword.value
    }

    // Move to DID password step
    wizard.currentStepIndex.value = 2
  } catch (error) {
    console.error('Recovery login failed:', error)
    add({
      title: t('errors.loginFailed'),
      description: error instanceof Error ? error.message : 'Unknown error',
      color: 'error',
    })
  } finally {
    wizard.isLoading.value = false
  }
}

const clearForm = async () => {
  wizard.reset()
}

defineExpose({
  clearForm,
  currentStepIndex: wizard.currentStepIndex,
})
</script>

<i18n lang="yaml">
de:
  steps:
    loginEmail:
      title: E-Mail
    loginOtp:
      title: Code bestätigen
    didPassword:
      title: Identität entschlüsseln
      description: Gib das Passwort ein, mit dem deine Identität verschlüsselt wurde.
      label: Identitäts-Passwort
    selectVault:
      title: Vault auswählen
      description: Wähle einen Vault, den du synchronisieren möchtest
      encryptedVault: Verschlüsselter Vault
      createdAt: Erstellt am
      noVaults: Keine Vaults gefunden
      createNew: Neuen Vault erstellen
      createNewDescription: Erstelle einen neuen Vault auf dem Server
      vaultName: Lokaler Vault-Name
      vaultNameDescription: Name unter dem der Vault lokal gespeichert wird
      vaultNameExists: Ein Vault mit diesem Namen existiert bereits
      vaultPassword: Vault-Passwort
      vaultPasswordDescription: Das Vault-Passwort unterscheidet sich von deinem Identitäts-Passwort
      vaultPasswordDescriptionNew: Wähle ein sicheres Passwort für deinen Vault
      confirmPassword: Passwort bestätigen
      confirmPasswordDescription: Bestätige dein Vault-Passwort
      passwordMismatch: Passwörter stimmen nicht überein
  actions:
    back: Zurück
    next: Weiter
    complete: Abschließen
    open: Öffnen
    cancel: Abbrechen
  errors:
    serverConnection: Verbindung zum Server fehlgeschlagen
    loginFailed: Anmeldung fehlgeschlagen
    loadVaultsFailed: Vaults konnten nicht geladen werden
    vaultSelectionRequired: Bitte wähle einen Vault aus
    wrongPassword: Falsches Passwort – Vault konnte nicht entschlüsselt werden
    wrongDidPassword: Falsches Passwort – Identität konnte nicht entschlüsselt werden
  validation:
    originUrlRequired: Server-URL ist erforderlich
    originUrlInvalid: Muss eine gültige URL sein
    vaultNameRequired: Vault-Name ist erforderlich
    vaultNameTooLong: Vault-Name ist zu lang (max. 255 Zeichen)
    vaultPasswordMinLength: Passwort muss mindestens 6 Zeichen lang sein
    vaultPasswordTooLong: Passwort ist zu lang (max. 255 Zeichen)
en:
  steps:
    loginEmail:
      title: Email
    loginOtp:
      title: Verify Code
    didPassword:
      title: Decrypt Identity
      description: Enter the password used to encrypt your identity.
      label: Identity Password
    selectVault:
      title: Select Vault
      description: Choose a vault you want to synchronize
      encryptedVault: Encrypted Vault
      createdAt: Created at
      noVaults: No vaults found
      createNew: Create new vault
      createNewDescription: Create a new vault on the server
      vaultName: Local Vault Name
      vaultNameDescription: Name under which the vault will be stored locally
      vaultNameExists: A vault with this name already exists
      vaultPassword: Vault Password
      vaultPasswordDescription: The vault password differs from your identity password
      vaultPasswordDescriptionNew: Choose a secure password for your vault
      confirmPassword: Confirm password
      confirmPasswordDescription: Confirm your vault password
      passwordMismatch: Passwords do not match
  actions:
    back: Back
    next: Next
    complete: Complete
    open: Open
    cancel: Cancel
  errors:
    serverConnection: Failed to connect to server
    loginFailed: Login failed
    loadVaultsFailed: Failed to load vaults
    vaultSelectionRequired: Please select a vault
    wrongPassword: Wrong password — could not decrypt vault
    wrongDidPassword: Wrong password — could not decrypt identity
  validation:
    originUrlRequired: Server URL is required
    originUrlInvalid: Must be a valid URL
    vaultNameRequired: Vault name is required
    vaultNameTooLong: Vault name is too long (max. 255 characters)
    vaultPasswordMinLength: Password must be at least 6 characters
    vaultPasswordTooLong: Password is too long (max. 255 characters)
</i18n>
