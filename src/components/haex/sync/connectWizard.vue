<template>
  <div class="space-y-6">
    <!-- Stepper -->
    <UStepper
      v-model="currentStepIndex"
      :items="steps"
      :linear="false"
    >
      <template #loginEmail>
        <div class="space-y-4">
          <HaexSyncRecoveryLogin @otp-requested="onOtpRequested" />
        </div>
      </template>

      <template #loginOtp>
        <div class="space-y-4">
          <HaexSyncRecoveryLoginOtp
            :server-url="otpServerUrl"
            :email="otpEmail"
            @recovered="onRecoveryComplete"
            @change-email="currentStepIndex = 0"
          />
        </div>
      </template>

      <template #didPassword>
        <div class="space-y-4">
          <p class="text-sm text-muted">
            {{ t('steps.didPassword.description') }}
          </p>

          <UiInputPassword
            v-model="didPassword"
            :label="t('steps.didPassword.label')"
            leading-icon="i-lucide-fingerprint"
            size="lg"
            class="w-full"
          />

          <p
            v-if="didPasswordError"
            class="text-sm text-error"
          >
            {{ didPasswordError }}
          </p>
        </div>
      </template>

      <template #selectVault>
        <div class="space-y-4">
          <p class="text-sm text-muted">
            {{ t('steps.selectVault.description') }}
          </p>

          <!-- Loading state -->
          <div
            v-if="isLoadingVaults"
            class="flex items-center justify-center p-8"
          >
            <span class="loading loading-spinner loading-lg" />
          </div>

          <!-- Vault list -->
          <div
            v-else
            class="space-y-2 px-1"
          >
            <div
              v-for="vault in availableVaults"
              :key="vault.vaultId"
              class="card bg-elevated rounded-lg p-4 cursor-pointer hover:bg-muted transition-colors"
              :class="{
                'ring-2 ring-primary':
                  selectedVaultId === vault.vaultId && !isCreatingNewVault,
                'ring-2 ring-error':
                  step3Error && !selectedVaultId && !isCreatingNewVault,
              }"
              @click="selectVault(vault.vaultId)"
            >
              <div class="flex items-center justify-between">
                <div>
                  <p class="font-medium">
                    {{
                      decryptedVaultNames[vault.vaultId] ||
                      t('steps.selectVault.encryptedVault')
                    }}
                  </p>
                  <p class="text-sm text-muted">
                    {{ t('steps.selectVault.createdAt') }}:
                    {{ formatDate(vault.createdAt) }}
                  </p>
                </div>
                <div
                  v-if="
                    selectedVaultId === vault.vaultId && !isCreatingNewVault
                  "
                >
                  <span
                    v-if="isCheckingVaultPassword"
                    class="loading loading-spinner loading-sm"
                  />
                  <i
                    v-else-if="vaultPasswordVerified"
                    class="i-lucide-check-circle text-2xl text-primary"
                  />
                  <i
                    v-else-if="needsVaultPassword"
                    class="i-lucide-lock text-2xl text-warning"
                  />
                </div>
              </div>
            </div>

            <!-- Create new vault option -->
            <div
              class="card bg-elevated rounded-lg p-4 cursor-pointer hover:bg-muted transition-colors"
              :class="{
                'ring-2 ring-primary': isCreatingNewVault,
              }"
              @click="selectNewVault()"
            >
              <div class="flex items-center justify-between">
                <div>
                  <p class="font-medium">
                    {{ t('steps.selectVault.createNew') }}
                  </p>
                  <p class="text-sm text-muted">
                    {{ t('steps.selectVault.createNewDescription') }}
                  </p>
                </div>
                <div
                  v-if="isCreatingNewVault"
                  class="text-primary"
                >
                  <i class="i-lucide-check-circle text-2xl" />
                </div>
              </div>
            </div>

            <!-- Error message -->
            <p
              v-if="step3Error"
              class="text-sm text-error mt-2"
            >
              {{ step3Error }}
            </p>
          </div>

          <!-- Local vault name (always shown when vault selected) -->
          <div
            v-if="selectedVaultId || isCreatingNewVault"
            class="space-y-4 pt-2"
          >
            <UiInput
              v-model="localVaultName"
              v-model:errors="step3Errors.vaultName"
              :label="t('steps.selectVault.vaultName')"
              :description="t('steps.selectVault.vaultNameDescription')"
              :schema="wizardSchema.vaultName"
              :check="check"
              size="lg"
              class="w-full"
              @blur="checkVaultNameExistsAsync"
            />
            <p
              v-if="vaultNameExists"
              class="text-sm text-error -mt-3"
            >
              {{ t('steps.selectVault.vaultNameExists') }}
            </p>

            <!-- Vault password: shown for new vaults or when DID password didn't match -->
            <template v-if="needsVaultPassword || isCreatingNewVault">
              <UiInputPassword
                v-model="vaultPassword"
                v-model:errors="step3Errors.password"
                :label="t('steps.selectVault.vaultPassword')"
                :description="
                  isCreatingNewVault
                    ? t('steps.selectVault.vaultPasswordDescriptionNew')
                    : t('steps.selectVault.vaultPasswordDescription')
                "
                :schema="wizardSchema.vaultPassword"
                :check="check"
                leading-icon="i-lucide-lock"
                size="lg"
                class="w-full"
              />

              <!-- Password confirmation for new vault -->
              <UiInputPassword
                v-if="isCreatingNewVault"
                v-model="vaultPasswordConfirm"
                v-model:errors="step3Errors.passwordConfirm"
                :label="t('steps.selectVault.confirmPassword')"
                :description="t('steps.selectVault.confirmPasswordDescription')"
                :schema="wizardSchema.vaultPassword"
                :check="check"
                leading-icon="i-lucide-lock"
                size="lg"
                class="w-full"
              />
              <p
                v-if="
                  isCreatingNewVault &&
                  vaultPasswordConfirm &&
                  vaultPassword !== vaultPasswordConfirm
                "
                class="text-sm text-error -mt-3"
              >
                {{ t('steps.selectVault.passwordMismatch') }}
              </p>
            </template>
          </div>
        </div>
      </template>
    </UStepper>

    <!-- Actions -->
    <div class="flex gap-3 mt-6">
      <UButton
        color="neutral"
        variant="outline"
        size="lg"
        @click="cancel"
      >
        {{ t('actions.cancel') }}
      </UButton>
      <UButton
        v-if="currentStepIndex > 0"
        color="neutral"
        variant="outline"
        size="lg"
        @click="previousStep"
      >
        {{ t('actions.back') }}
      </UButton>
      <div class="flex-1" />
      <UButton
        v-if="currentStepIndex < 3"
        color="primary"
        size="lg"
        :disabled="!canProceed"
        :loading="isLoading"
        @click="nextStep"
      >
        {{ t('actions.next') }}
      </UButton>
      <UButton
        v-else
        color="primary"
        size="lg"
        :disabled="!canComplete || isCheckingVaultPassword"
        :loading="isLoading || isCheckingVaultPassword"
        @click="completeSetupAsync"
      >
        {{ vaultPasswordVerified ? t('actions.open') : t('actions.complete') }}
      </UButton>
    </div>
  </div>
</template>

<script setup lang="ts">
import { createClient } from '@supabase/supabase-js'
import {
  decryptWithPrivateKeyAsync,
  decryptPrivateKeyAsync,
  decryptVaultKey,
} from '@haex-space/vault-sdk'
import type { StepperItem } from '@nuxt/ui'
import type { AppSupabaseClient } from '~/stores/sync/engine/supabase'
import { createConnectWizardSchema } from './connectWizardSchema'
import type { RecoveryKeyData } from '~/composables/useIdentityRecovery'

const { t } = useI18n()
const { add } = useToast()
const { decryptAndVerifyAsync } = useIdentityRecovery()

// Create validation schema with i18n
const wizardSchema = computed(() => createConnectWizardSchema(t))

interface VaultInfo {
  vaultId: string
  encryptedVaultName: string
  vaultNameNonce: string
  ephemeralPublicKey: string
  createdAt: string
}

defineProps<{
  showCancel?: boolean
}>()

const emit = defineEmits<{
  complete: [
    {
      backendId: string
      vaultId: string
      vaultName: string
      localVaultName: string
      serverUrl: string
      identityId: string
      identityPublicKey: string
      vaultPassword: string
      isNewVault: boolean
    },
  ]
  cancel: []
}>()

// Stepper state
const currentStepIndex = ref(0)

// OTP step data (passed from email step)
const otpServerUrl = ref('')
const otpEmail = ref('')

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

const isLoading = ref(false)
const check = ref(false)

const { currentVaultPassword } = storeToRefs(useVaultStore())

// Step 1: Identity Auth (via Recovery)
const credentials = ref({
  serverUrl: 'https://sync.haex.space',
  identityId: '',
})
const supabaseClient = shallowRef<AppSupabaseClient | null>(null)

// Recovery mode: stores encrypted private key data from OTP verification
const recoveredKeyData = ref<RecoveryKeyData | null>(null)

// Step 2: DID Password (decrypt identity private key)
const didPassword = ref('')
const didPasswordError = ref('')
const decryptedPrivateKey = ref<string | null>(null)

// Step 3: Select Vault + optional vault password
const availableVaults = ref<VaultInfo[]>([])
const selectedVaultId = ref<string | null>(null)
const isLoadingVaults = ref(false)
const step3Error = ref('')
const isCreatingNewVault = ref(false)
const decryptedVaultNames = ref<Record<string, string>>({})
const needsVaultPassword = ref(false)
const isCheckingVaultPassword = ref(false)
const vaultPasswordVerified = ref(false)
const localVaultName = ref('')
const vaultNameExists = ref(false)
const vaultPassword = ref('')
const vaultPasswordConfirm = ref('')
const step3Errors = reactive({
  vaultName: [] as string[],
  password: [] as string[],
  passwordConfirm: [] as string[],
})

// Computed for step validation
const canProceed = computed(() => {
  if (currentStepIndex.value === 2) {
    return didPassword.value.length > 0
  }
  return false
})

const canComplete = computed(() => {
  // Must have a vault selected or creating new
  if (!selectedVaultId.value && !isCreatingNewVault.value) return false
  // Must have a local vault name
  if (!localVaultName.value || vaultNameExists.value) return false
  if (step3Errors.vaultName.length > 0) return false

  // For new vault: need password + confirmation
  if (isCreatingNewVault.value) {
    return (
      vaultPassword.value !== '' &&
      vaultPasswordConfirm.value !== '' &&
      vaultPassword.value === vaultPasswordConfirm.value &&
      step3Errors.password.length === 0
    )
  }

  // For existing vault with separate password: need password
  if (needsVaultPassword.value) {
    return vaultPassword.value !== '' && step3Errors.password.length === 0
  }

  // For existing vault: DID password must be verified
  return vaultPasswordVerified.value
})

// Keyboard shortcuts handlers
whenever(escape, () => {
  cancel()
})

whenever(enter, () => {
  if (currentStepIndex.value < 3 && canProceed.value && !isLoading.value) {
    nextStep()
  } else if (
    currentStepIndex.value === 3 &&
    canComplete.value &&
    !isLoading.value
  ) {
    completeSetupAsync()
  }
})

// Methods
const onOtpRequested = (data: { serverUrl: string; email: string }) => {
  otpServerUrl.value = data.serverUrl
  otpEmail.value = data.email
  currentStepIndex.value = 1
}

const nextStep = async () => {
  // Step 2: DID password → decrypt private key, load & decrypt vault names
  if (currentStepIndex.value === 2) {
    if (!recoveredKeyData.value) return
    isLoading.value = true
    didPasswordError.value = ''

    try {
      const valid = await decryptAndVerifyAsync(
        recoveredKeyData.value,
        didPassword.value,
      )
      if (!valid) {
        didPasswordError.value = t('errors.wrongDidPassword')
        return
      }

      // Decrypt private key and store for vault name decryption
      decryptedPrivateKey.value = await decryptPrivateKeyAsync(
        recoveredKeyData.value.encryptedPrivateKey,
        recoveredKeyData.value.privateKeyNonce,
        recoveredKeyData.value.privateKeySalt,
        didPassword.value,
      )

      // Load vaults and decrypt names
      await loadVaultsAsync()
      await decryptVaultNamesAsync(decryptedPrivateKey.value)

      currentStepIndex.value++
    } catch {
      didPasswordError.value = t('errors.wrongDidPassword')
    } finally {
      isLoading.value = false
    }
  }
}

const previousStep = () => {
  currentStepIndex.value--
}

const loadVaultsAsync = async () => {
  if (!supabaseClient.value) return

  isLoadingVaults.value = true

  try {
    // Get auth token
    const {
      data: { session },
    } = await supabaseClient.value.auth.getSession()
    if (!session?.access_token) {
      throw new Error('Not authenticated')
    }

    // Fetch vaults from server
    const response = await fetch(`${credentials.value.serverUrl}/sync/vaults`, {
      method: 'GET',
      headers: {
        Authorization: `Bearer ${session.access_token}`,
      },
    })

    if (!response.ok) {
      throw new Error('Failed to fetch vaults')
    }

    const data = await response.json()
    availableVaults.value = data.vaults
  } catch (error) {
    console.error('Failed to load vaults:', error)
    add({
      title: t('errors.loadVaultsFailed'),
      description: error instanceof Error ? error.message : 'Unknown error',
      color: 'error',
    })
  } finally {
    isLoadingVaults.value = false
  }
}

const decryptVaultNamesAsync = async (privateKeyBase64: string) => {
  const names: Record<string, string> = {}
  for (const vault of availableVaults.value) {
    try {
      const decryptedBytes = await decryptWithPrivateKeyAsync(
        {
          encryptedData: vault.encryptedVaultName,
          nonce: vault.vaultNameNonce,
          ephemeralPublicKey: vault.ephemeralPublicKey,
        },
        privateKeyBase64,
      )
      names[vault.vaultId] = new TextDecoder().decode(decryptedBytes)
    } catch {
      // Decryption failed — keep showing fallback
    }
  }
  decryptedVaultNames.value = names
}

const checkVaultNameExistsAsync = async () => {
  if (!localVaultName.value) {
    vaultNameExists.value = false
    return
  }

  try {
    const vaultStore = useVaultStore()
    const exists = await vaultStore.vaultExistsAsync(localVaultName.value)
    vaultNameExists.value = exists
  } catch (error) {
    console.error('Failed to check vault name:', error)
    vaultNameExists.value = false
  }
}

const completeSetupAsync = async () => {
  check.value = true
  await nextTick()

  if (!canComplete.value) return
  if (!isCreatingNewVault.value && !selectedVaultId.value) return

  if (!supabaseClient.value) {
    throw new Error('Supabase client not initialized')
  }

  // Determine effective vault password
  const effectivePassword = needsVaultPassword.value || isCreatingNewVault.value
    ? vaultPassword.value
    : didPassword.value

  // Store Supabase client in syncEngineStore for later use
  const backendId = crypto.randomUUID()
  const syncEngineStore = useSyncEngineStore()
  syncEngineStore.setSupabaseClient(supabaseClient.value, backendId)

  if (isCreatingNewVault.value) {
    emit('complete', {
      backendId,
      vaultId: crypto.randomUUID(),
      vaultName: localVaultName.value,
      localVaultName: localVaultName.value,
      serverUrl: credentials.value.serverUrl,
      identityId: credentials.value.identityId,
      identityPublicKey: recoveredKeyData.value!.publicKey,
      vaultPassword: effectivePassword,
      isNewVault: true,
    })
  } else {
    const selectedVault = availableVaults.value.find(
      (v) => v.vaultId === selectedVaultId.value,
    )
    if (!selectedVault) return

    emit('complete', {
      backendId,
      vaultId: selectedVault.vaultId,
      vaultName: localVaultName.value,
      localVaultName: localVaultName.value,
      serverUrl: credentials.value.serverUrl,
      identityId: credentials.value.identityId,
      identityPublicKey: recoveredKeyData.value!.publicKey,
      vaultPassword: effectivePassword,
      isNewVault: false,
    })
  }
}

const selectVault = async (vaultId: string) => {
  selectedVaultId.value = vaultId
  isCreatingNewVault.value = false
  needsVaultPassword.value = false
  vaultPasswordVerified.value = false
  vaultPassword.value = ''
  step3Error.value = ''

  // Auto-fill local vault name with decrypted name
  localVaultName.value = decryptedVaultNames.value[vaultId] || 'HaexVault'
  checkVaultNameExistsAsync()

  // Try DID password as vault password in background
  await tryDIDPasswordAsVaultPasswordAsync(vaultId)
}

const tryDIDPasswordAsVaultPasswordAsync = async (vaultId: string) => {
  if (!supabaseClient.value) return

  isCheckingVaultPassword.value = true

  try {
    const { data: { session } } = await supabaseClient.value.auth.getSession()
    if (!session?.access_token) return

    // Fetch encrypted vault key from server
    const response = await fetch(
      `${credentials.value.serverUrl}/sync/vault-key/${vaultId}`,
      {
        method: 'GET',
        headers: { Authorization: `Bearer ${session.access_token}` },
      },
    )

    if (!response.ok) return

    const data = await response.json()

    // Try decrypting vault key with DID password
    await decryptVaultKey(
      data.vaultKey.encryptedVaultKey,
      data.vaultKey.vaultKeySalt,
      data.vaultKey.vaultKeyNonce,
      didPassword.value,
    )

    // Success — DID password works as vault password
    vaultPasswordVerified.value = true
  } catch (error) {
    // OperationError = wrong password → show vault password field
    if (error instanceof Error && error.name === 'OperationError') {
      needsVaultPassword.value = true
    }
  } finally {
    isCheckingVaultPassword.value = false
  }
}

const selectNewVault = () => {
  isCreatingNewVault.value = true
  selectedVaultId.value = null
  needsVaultPassword.value = false
  vaultPassword.value = ''
  vaultPasswordConfirm.value = ''
  step3Error.value = ''
  localVaultName.value = 'HaexVault'
  checkVaultNameExistsAsync()
}

const cancel = () => {
  emit('cancel')
}

const onRecoveryComplete = async (data: {
  serverUrl: string
  recoveryKeyData: RecoveryKeyData
  session: {
    access_token: string
    refresh_token: string
    expires_in: number
    expires_at: number
  }
  identity: { id: string; did: string; tier: string }
}) => {
  isLoading.value = true

  try {
    credentials.value.serverUrl = data.serverUrl
    credentials.value.identityId = data.identity.id
    recoveredKeyData.value = data.recoveryKeyData

    // Connect to server and get Supabase config
    const response = await fetch(data.serverUrl)
    if (!response.ok) throw new Error(t('errors.serverConnection'))
    const serverInfo = await response.json()

    // Create Supabase client with the session from recovery
    // Disable auto-refresh and persistence — this is a temporary client for the wizard only
    supabaseClient.value = createClient(
      serverInfo.supabaseUrl,
      serverInfo.supabaseAnonKey,
      {
        auth: {
          autoRefreshToken: false,
          persistSession: false,
        },
      },
    )
    await supabaseClient.value.auth.setSession({
      access_token: data.session.access_token,
      refresh_token: data.session.refresh_token,
    })

    // Pre-fill DID password with current vault password if available
    if (currentVaultPassword.value) {
      didPassword.value = currentVaultPassword.value
    }

    // Move to DID password step
    currentStepIndex.value = 2
  } catch (error) {
    console.error('Recovery login failed:', error)
    add({
      title: t('errors.loginFailed'),
      description: error instanceof Error ? error.message : 'Unknown error',
      color: 'error',
    })
  } finally {
    isLoading.value = false
  }
}

const clearForm = () => {
  currentStepIndex.value = 0
  otpServerUrl.value = ''
  otpEmail.value = ''
  credentials.value = {
    serverUrl: 'https://sync.haex.space',
    identityId: '',
  }
  availableVaults.value = []
  selectedVaultId.value = null
  isCreatingNewVault.value = false
  decryptedVaultNames.value = {}
  recoveredKeyData.value = null
  didPassword.value = ''
  didPasswordError.value = ''
  decryptedPrivateKey.value = null
  needsVaultPassword.value = false
  isCheckingVaultPassword.value = false
  vaultPasswordVerified.value = false
  localVaultName.value = ''
  vaultPassword.value = ''
  vaultPasswordConfirm.value = ''
  vaultNameExists.value = false
  supabaseClient.value = null
}

const formatDate = (dateStr: string) => {
  return new Date(dateStr).toLocaleDateString()
}

defineExpose({
  clearForm,
  currentStepIndex,
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
    serverUrlRequired: Server-URL ist erforderlich
    serverUrlInvalid: Muss eine gültige URL sein
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
    serverUrlRequired: Server URL is required
    serverUrlInvalid: Must be a valid URL
    vaultNameRequired: Vault name is required
    vaultNameTooLong: Vault name is too long (max. 255 characters)
    vaultPasswordMinLength: Password must be at least 6 characters
    vaultPasswordTooLong: Password is too long (max. 255 characters)
</i18n>
