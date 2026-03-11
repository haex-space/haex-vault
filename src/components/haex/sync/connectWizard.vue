<template>
  <div class="space-y-6">
    <!-- Stepper -->
    <UStepper
      v-model="currentStepIndex"
      :items="steps"
      class="mb-6"
    />

    <!-- Step Content -->
    <div>
      <!-- Step 1: Identity Auth -->
      <div
        v-if="currentStepIndex === 0"
        class="space-y-4"
      >
        <!-- Mode Toggle -->
        <div class="flex gap-2 mb-4">
          <UButton
            :color="!isRecoveryMode ? 'primary' : 'neutral'"
            :variant="!isRecoveryMode ? 'solid' : 'outline'"
            size="sm"
            @click="isRecoveryMode = false"
          >
            {{ t('steps.login.modeLocal') }}
          </UButton>
          <UButton
            :color="isRecoveryMode ? 'primary' : 'neutral'"
            :variant="isRecoveryMode ? 'solid' : 'outline'"
            size="sm"
            @click="isRecoveryMode = true"
          >
            {{ t('steps.login.modeRecovery') }}
          </UButton>
        </div>

        <!-- Standard: Local Identity -->
        <HaexSyncAddBackend
          v-if="!isRecoveryMode"
          ref="connectRef"
          v-model:server-url="credentials.serverUrl"
          v-model:identity-id="credentials.identityId"
          v-model:approved-claims="credentials.approvedClaims"
          :items="serverOptions"
          :is-loading="isLoading"
          autofocus
        />

        <!-- Recovery: Email + OTP -->
        <HaexSyncRecoveryLogin
          v-else
          @recovered="onRecoveryComplete"
        />
      </div>

      <!-- Step 2: Select Vault -->
      <div
        v-else-if="currentStepIndex === 1"
        class="space-y-4"
      >
        <p class="text-sm text-muted">
          {{ t('steps.selectVault.description') }}
        </p>

        <!-- Loading state -->
        <div
          v-if="isLoadingVaults"
          class="flex items-center justify-center p-8"
        >
          <span class="loading loading-spinner loading-lg"/>
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
              'ring-2 ring-primary': selectedVaultId === vault.vaultId && !isCreatingNewVault,
              'ring-2 ring-error': step2Error && !selectedVaultId && !isCreatingNewVault,
            }"
            @click="selectedVaultId = vault.vaultId; isCreatingNewVault = false; step2Error = ''"
          >
            <div class="flex items-center justify-between">
              <div>
                <p class="font-medium">
                  {{ t('steps.selectVault.encryptedVault') }}
                </p>
                <p class="text-sm text-muted">
                  {{ t('steps.selectVault.createdAt') }}:
                  {{ formatDate(vault.createdAt) }}
                </p>
              </div>
              <div
                v-if="selectedVaultId === vault.vaultId && !isCreatingNewVault"
                class="text-primary"
              >
                <i class="i-lucide-check-circle text-2xl"/>
              </div>
            </div>
          </div>

          <!-- Create new vault option -->
          <div
            class="card bg-elevated rounded-lg p-4 cursor-pointer hover:bg-muted transition-colors"
            :class="{
              'ring-2 ring-primary': isCreatingNewVault,
            }"
            @click="isCreatingNewVault = true; selectedVaultId = null; step2Error = ''"
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
                <i class="i-lucide-check-circle text-2xl"/>
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

      <!-- Step 3: Enter Vault Password -->
      <div
        v-else-if="currentStepIndex === 2"
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
            :description="isCreatingNewVault ? t('steps.enterVaultPassword.vaultPasswordDescriptionNew') : t('steps.enterVaultPassword.vaultPasswordDescription')"
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
            :description="t('steps.enterVaultPassword.confirmPasswordDescription')"
            :schema="wizardSchema.vaultPassword"
            :check="check"
            leading-icon="i-lucide-lock"
            size="lg"
            class="w-full"
          />
          <p
            v-if="isCreatingNewVault && vaultPasswordConfirm && vaultPassword !== vaultPasswordConfirm"
            class="text-sm text-error -mt-3"
          >
            {{ t('steps.enterVaultPassword.passwordMismatch') }}
          </p>
        </div>
      </div>
    </div>

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
      <div class="flex-1"/>
      <UButton
        v-if="currentStepIndex < 2"
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
import type { AppSupabaseClient } from '~/stores/sync/engine/supabase'
import { createConnectWizardSchema } from './connectWizardSchema'

const { t } = useI18n()
const { serverOptions } = useSyncServerOptions()
const { add } = useToast()

// Create validation schema with i18n
const wizardSchema = computed(() => createConnectWizardSchema(t))

const { loginAsync: challengeLoginAsync } = useCreateSyncConnection()

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

// Keyboard shortcuts with VueUse
const keys = useMagicKeys()
const escape = computed(() => keys.escape?.value ?? false)
const enter = computed(() => keys.enter?.value ?? false)

// Stepper state
const currentStepIndex = ref(0)
const steps = computed(() => [
  {
    label: t('steps.login.title'),
    icon: 'i-lucide-log-in',
  },
  {
    label: t('steps.selectVault.title'),
    icon: 'i-lucide-folder',
  },
  {
    label: t('steps.enterVaultPassword.title'),
    icon: 'i-lucide-key',
  },
])

const isLoading = ref(false)
const check = ref(false)

// Step 1: Identity Auth
const credentials = ref({
  serverUrl: 'https://sync.haex.space',
  identityId: '',
  approvedClaims: {} as Record<string, string>,
})
const supabaseClient = shallowRef<AppSupabaseClient | null>(null)

const isLoginFormValid = computed(() => {
  return (
    credentials.value.serverUrl !== '' &&
    credentials.value.identityId !== ''
  )
})

// Step 2: Select Vault
const availableVaults = ref<VaultInfo[]>([])
const selectedVaultId = ref<string | null>(null)
const isLoadingVaults = ref(false)
const step2Error = ref('')
const isCreatingNewVault = ref(false)

// Recovery mode
const isRecoveryMode = ref(false)
const recoveredVaultPassword = ref('')

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
  if (currentStepIndex.value === 0) {
    // In recovery mode, progression is handled by RecoveryLogin component
    if (isRecoveryMode.value) return false
    return isLoginFormValid.value
  }
  if (currentStepIndex.value === 1) {
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
  if (currentStepIndex.value < 2 && canProceed.value && !isLoading.value) {
    nextStep()
  } else if (currentStepIndex.value === 2 && isStep3Valid.value && !isLoading.value) {
    completeSetupAsync()
  }
})

// Methods
const nextStep = async () => {
  if (currentStepIndex.value === 0) {
    await loginAsync()
  } else if (currentStepIndex.value === 1) {
    // Validate Step 2 (vault selection or new vault)
    if (!selectedVaultId.value && !isCreatingNewVault.value) {
      step2Error.value = t('errors.vaultSelectionRequired')
      return
    }

    if (isCreatingNewVault.value) {
      // New vault: set default vault name
      localVaultName.value = 'HaexVault'
      vaultPasswordConfirm.value = ''
    } else {
      // Existing vault: use vault ID as placeholder name
      localVaultName.value = 'HaexVault'
      // Check if this name already exists locally
      await checkVaultNameExistsAsync()
    }

    currentStepIndex.value++

    // Pre-fill vault password from recovery if available
    if (recoveredVaultPassword.value) {
      vaultPassword.value = recoveredVaultPassword.value
    }
  }
}

const previousStep = () => {
  if (currentStepIndex.value > 0) {
    currentStepIndex.value--
  }
}

const loginAsync = async () => {
  if (!isLoginFormValid.value) return

  isLoading.value = true

  try {
    // 1. Connect to server and get Supabase config
    const response = await fetch(credentials.value.serverUrl)
    if (!response.ok) {
      throw new Error(t('errors.serverConnection'))
    }

    const serverInfo = await response.json()

    // 2. Create Supabase client
    supabaseClient.value = createClient(
      serverInfo.supabaseUrl,
      serverInfo.supabaseAnonKey,
    )

    // 3. Challenge-response login
    const session = await challengeLoginAsync(
      credentials.value.serverUrl,
      credentials.value.identityId,
    )

    // 4. Set session
    await supabaseClient.value.auth.setSession({
      access_token: session.access_token,
      refresh_token: session.refresh_token,
    })

    // 5. Load available vaults
    await loadVaultsAsync()

    // 6. Move to next step
    currentStepIndex.value = 1
  } catch (error) {
    console.error('Login failed:', error)
    add({
      title: t('errors.loginFailed'),
      description: error instanceof Error ? error.message : 'Unknown error',
      color: 'error',
    })
  } finally {
    isLoading.value = false
  }
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
    // Existing vault
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
      vaultPassword: vaultPassword.value,
      isNewVault: false,
    })
  }
}

const cancel = () => {
  emit('cancel')
}

const onRecoveryComplete = async (data: {
  identityId: string
  serverUrl: string
  vaultPassword: string
}) => {
  // Set credentials from recovery
  credentials.value.serverUrl = data.serverUrl
  credentials.value.identityId = data.identityId
  recoveredVaultPassword.value = data.vaultPassword

  // Continue with normal flow: login and load vaults
  await loginAsync()
}

const clearForm = () => {
  currentStepIndex.value = 0
  credentials.value = {
    serverUrl: 'https://sync.haex.space',
    identityId: '',
    approvedClaims: {},
  }
  availableVaults.value = []
  selectedVaultId.value = null
  isCreatingNewVault.value = false
  isRecoveryMode.value = false
  recoveredVaultPassword.value = ''
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
})
</script>

<i18n lang="yaml">
de:
  steps:
    login:
      title: Verbinden
      modeLocal: Identität vorhanden
      modeRecovery: Per E-Mail wiederherstellen
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
  validation:
    serverUrlRequired: Server-URL ist erforderlich
    serverUrlInvalid: Muss eine gültige URL sein
    vaultNameRequired: Vault-Name ist erforderlich
    vaultNameTooLong: Vault-Name ist zu lang (max. 255 Zeichen)
    vaultPasswordMinLength: Passwort muss mindestens 6 Zeichen lang sein
    vaultPasswordTooLong: Passwort ist zu lang (max. 255 Zeichen)
en:
  steps:
    login:
      title: Connect
      modeLocal: Identity available
      modeRecovery: Recover via email
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
  validation:
    serverUrlRequired: Server URL is required
    serverUrlInvalid: Must be a valid URL
    vaultNameRequired: Vault name is required
    vaultNameTooLong: Vault name is too long (max. 255 characters)
    vaultPasswordMinLength: Password must be at least 6 characters
    vaultPasswordTooLong: Password is too long (max. 255 characters)
</i18n>
