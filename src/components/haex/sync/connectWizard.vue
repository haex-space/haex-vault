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
                  step2Error && !selectedVaultId && !isCreatingNewVault,
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
                  class="text-primary"
                >
                  <i class="i-lucide-check-circle text-2xl" />
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
              v-if="step2Error"
              class="text-sm text-error mt-2"
            >
              {{ step2Error }}
            </p>
          </div>
        </div>
      </template>

      <template #vaultPassword>
        <div
          ref="step3Container"
          class="space-y-4"
        >
          <p class="text-sm text-muted">
            {{ t('steps.enterVaultPassword.description') }}
          </p>

          <div class="space-y-4">
            <UiInput
              v-model="localVaultName"
              v-model:errors="step3Errors.vaultName"
              :label="t('steps.enterVaultPassword.vaultName')"
              :description="t('steps.enterVaultPassword.vaultNameDescription')"
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
              {{ t('steps.enterVaultPassword.vaultNameExists') }}
            </p>

            <UiInputPassword
              v-model="vaultPassword"
              v-model:errors="step3Errors.password"
              :label="t('steps.enterVaultPassword.vaultPassword')"
              :description="
                isCreatingNewVault
                  ? t('steps.enterVaultPassword.vaultPasswordDescriptionNew')
                  : t('steps.enterVaultPassword.vaultPasswordDescription')
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
              :label="t('steps.enterVaultPassword.confirmPassword')"
              :description="
                t('steps.enterVaultPassword.confirmPasswordDescription')
              "
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
              {{ t('steps.enterVaultPassword.passwordMismatch') }}
            </p>
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
        :disabled="!isStep3Valid || isLoading"
        :loading="isLoading"
        @click="completeSetupAsync"
      >
        {{ t('actions.complete') }}
      </UButton>
    </div>
  </div>
</template>

<script setup lang="ts">
import { createClient } from '@supabase/supabase-js'
import {
  decryptString,
  deriveKeyFromPassword,
  base64ToArrayBuffer,
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
  vaultNameSalt: string
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
      vaultPassword: string
      isNewVault: boolean
    },
  ]
  cancel: []
}>()

// Template refs
const step3Container = useTemplateRef<HTMLElement>('step3Container')

// Stepper state
const currentStepIndex = ref(0)

// OTP step data (passed from email step)
const otpServerUrl = ref('')
const otpEmail = ref('')

// Keyboard shortcuts with VueUse
const keys = useMagicKeys()
const escape = computed(() => keys.escape?.value ?? false)
const enter = computed(() => keys.enter?.value ?? false)

// Auto-focus first input when entering step 3
watch(currentStepIndex, async (newIndex) => {
  if (newIndex === 3) {
    await nextTick()
    step3Container.value?.querySelector<HTMLInputElement>('input')?.focus()
  }
})
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
        slot: 'selectVault' as const,
        label: t('steps.selectVault.title'),
        icon: 'i-lucide-folder',
      },
      {
        slot: 'vaultPassword' as const,
        label: t('steps.enterVaultPassword.title'),
        icon: 'i-lucide-key',
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

// Step 2: Select Vault
const availableVaults = ref<VaultInfo[]>([])
const selectedVaultId = ref<string | null>(null)
const isLoadingVaults = ref(false)
const step2Error = ref('')
const isCreatingNewVault = ref(false)
const decryptedVaultNames = ref<Record<string, string>>({})

// Recovery mode: stores encrypted private key data from OTP verification
const recoveredKeyData = ref<RecoveryKeyData | null>(null)

// Step 3: Enter Vault Password
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
    return selectedVaultId.value !== null || isCreatingNewVault.value
  }
  return false
})

const isStep3Valid = computed(() => {
  const baseValid =
    localVaultName.value !== '' &&
    !vaultNameExists.value &&
    vaultPassword.value !== '' &&
    step3Errors.vaultName.length === 0 &&
    step3Errors.password.length === 0

  // For new vault: also check password confirmation
  if (isCreatingNewVault.value) {
    return (
      baseValid &&
      vaultPasswordConfirm.value !== '' &&
      vaultPassword.value === vaultPasswordConfirm.value
    )
  }

  return baseValid
})

// Keyboard shortcuts handlers
// ESC to cancel/close
whenever(escape, () => {
  cancel()
})

// Enter to proceed to next step
whenever(enter, () => {
  if (currentStepIndex.value < 3 && canProceed.value && !isLoading.value) {
    nextStep()
  } else if (
    currentStepIndex.value === 3 &&
    isStep3Valid.value &&
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
  if (currentStepIndex.value === 2) {
    // Validate Step 3 (vault selection or new vault)
    if (!selectedVaultId.value && !isCreatingNewVault.value) {
      step2Error.value = t('errors.vaultSelectionRequired')
      return
    }

    if (isCreatingNewVault.value) {
      localVaultName.value = 'HaexVault'
      vaultPasswordConfirm.value = ''
    } else {
      localVaultName.value =
        decryptedVaultNames.value[selectedVaultId.value!] || 'HaexVault'
      await checkVaultNameExistsAsync()
    }

    currentStepIndex.value++

    // Auto-attempt with current vault password — most users share vault and identity password.
    // Try silently; if it fails, show Step 4 for manual entry without an error toast.
    if (
      !isCreatingNewVault.value &&
      currentVaultPassword.value &&
      recoveredKeyData.value
    ) {
      const valid = await decryptAndVerifyAsync(
        recoveredKeyData.value,
        currentVaultPassword.value,
      )
      if (valid) {
        vaultPassword.value = currentVaultPassword.value
        await completeSetupAsync()
      }
      // Silently fall through to Step 4 on failure — user enters password manually
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

    // Vault names will be decrypted once the user enters their vault password in Step 3
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

const decryptVaultNamesAsync = async (password: string) => {
  const names: Record<string, string> = {}
  for (const vault of availableVaults.value) {
    try {
      const salt = base64ToArrayBuffer(vault.vaultNameSalt)
      const derivedKey = await deriveKeyFromPassword(password, salt)
      names[vault.vaultId] = await decryptString(
        vault.encryptedVaultName,
        vault.vaultNameNonce,
        derivedKey,
      )
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
  // Trigger validation for Step 3
  check.value = true

  // Wait for validation to complete
  await nextTick()

  // Check if validation passed
  if (!isStep3Valid.value) {
    return
  }

  // For existing vault: must have selectedVaultId
  if (!isCreatingNewVault.value && !selectedVaultId.value) return

  // Validate vault password against recovered private key (proves correct password)
  if (recoveredKeyData.value) {
    const valid = await decryptAndVerifyAsync(
      recoveredKeyData.value,
      vaultPassword.value,
    )
    if (!valid) {
      add({ title: t('errors.wrongPassword'), color: 'error' })
      return
    }
    // Decrypt vault names now that we have the correct password
    await decryptVaultNamesAsync(vaultPassword.value)
  }

  if (!supabaseClient.value) {
    throw new Error('Supabase client not initialized')
  }

  // Store Supabase client in syncEngineStore for later use
  // This is needed so ensureSyncKeyAsync can authenticate with the server
  const backendId = crypto.randomUUID()
  const syncEngineStore = useSyncEngineStore()
  syncEngineStore.setSupabaseClient(supabaseClient.value, backendId)

  if (isCreatingNewVault.value) {
    // New vault: generate new vaultId, use localVaultName as vault name
    emit('complete', {
      backendId,
      vaultId: crypto.randomUUID(),
      vaultName: localVaultName.value,
      localVaultName: localVaultName.value,
      serverUrl: credentials.value.serverUrl,
      identityId: credentials.value.identityId,
      vaultPassword: vaultPassword.value,
      isNewVault: true,
    })
  } else {
    // Existing vault: try to decrypt vault name with entered password
    const selectedVault = availableVaults.value.find(
      (v) => v.vaultId === selectedVaultId.value,
    )
    if (!selectedVault) return

    // Auto-fill vault name with decrypted name if user left the default placeholder
    const decryptedName = decryptedVaultNames.value[selectedVault.vaultId]
    if (decryptedName && localVaultName.value === 'HaexVault') {
      localVaultName.value = decryptedName
    }

    emit('complete', {
      backendId,
      vaultId: selectedVault.vaultId,
      vaultName: localVaultName.value,
      localVaultName: localVaultName.value,
      serverUrl: credentials.value.serverUrl,
      identityId: credentials.value.identityId,
      vaultPassword: vaultPassword.value,
      isNewVault: false,
    })
  }
}

const selectVault = (vaultId: string) => {
  selectedVaultId.value = vaultId
  isCreatingNewVault.value = false
  step2Error.value = ''
}

const selectNewVault = () => {
  isCreatingNewVault.value = true
  selectedVaultId.value = null
  step2Error.value = ''
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

    // Create Supabase client with the session from recovery (no challenge-response needed)
    supabaseClient.value = createClient(
      serverInfo.supabaseUrl,
      serverInfo.supabaseAnonKey,
    )
    await supabaseClient.value.auth.setSession({
      access_token: data.session.access_token,
      refresh_token: data.session.refresh_token,
    })

    // Load available vaults and move to vault selection step
    await loadVaultsAsync()
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
    selectVault:
      title: Vault auswählen
      description: Wähle einen Vault, den du synchronisieren möchtest
      encryptedVault: Verschlüsselter Vault
      createdAt: Erstellt am
      noVaults: Keine Vaults gefunden
      createNew: Neuen Vault erstellen
      createNewDescription: Erstelle einen neuen Vault auf dem Server
    enterVaultPassword:
      title: Vault-Passwort
      description: Gib das Passwort deines Vaults ein, um die Synchronisierung einzurichten
      vaultName: Lokaler Vault-Name
      vaultNameDescription: Gib einen Namen für deinen lokalen Vault ein
      vaultNameExists: Ein Vault mit diesem Namen existiert bereits
      vaultPassword: Vault-Passwort
      vaultPasswordDescription: Das Passwort, mit dem du deinen Vault ursprünglich erstellt hast
      vaultPasswordDescriptionNew: Wähle ein sicheres Passwort für deinen Vault
      confirmPassword: Passwort bestätigen
      confirmPasswordDescription: Bestätige dein Vault-Passwort
      passwordMismatch: Passwörter stimmen nicht überein
  actions:
    back: Zurück
    next: Weiter
    complete: Abschließen
    cancel: Abbrechen
  errors:
    serverConnection: Verbindung zum Server fehlgeschlagen
    loginFailed: Anmeldung fehlgeschlagen
    loadVaultsFailed: Vaults konnten nicht geladen werden
    vaultSelectionRequired: Bitte wähle einen Vault aus
    wrongPassword: Falsches Passwort – Vault konnte nicht entschlüsselt werden
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
    selectVault:
      title: Select Vault
      description: Choose a vault you want to synchronize
      encryptedVault: Encrypted Vault
      createdAt: Created at
      noVaults: No vaults found
      createNew: Create new vault
      createNewDescription: Create a new vault on the server
    enterVaultPassword:
      title: Vault Password
      description: Enter your vault password to set up synchronization
      vaultName: Local Vault Name
      vaultNameDescription: Enter a name for your local vault
      vaultNameExists: A vault with this name already exists
      vaultPassword: Vault Password
      vaultPasswordDescription: The password you used to originally create your vault
      vaultPasswordDescriptionNew: Choose a secure password for your vault
      confirmPassword: Confirm password
      confirmPasswordDescription: Confirm your vault password
      passwordMismatch: Passwords do not match
  actions:
    back: Back
    next: Next
    complete: Complete
    cancel: Cancel
  errors:
    serverConnection: Failed to connect to server
    loginFailed: Login failed
    loadVaultsFailed: Failed to load vaults
    vaultSelectionRequired: Please select a vault
    wrongPassword: Wrong password — could not decrypt vault
  validation:
    serverUrlRequired: Server URL is required
    serverUrlInvalid: Must be a valid URL
    vaultNameRequired: Vault name is required
    vaultNameTooLong: Vault name is too long (max. 255 characters)
    vaultPasswordMinLength: Password must be at least 6 characters
    vaultPasswordTooLong: Password is too long (max. 255 characters)
</i18n>
