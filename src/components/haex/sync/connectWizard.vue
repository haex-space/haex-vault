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
      <!-- Step 1: Login -->
      <div
        v-if="currentStepIndex === 0"
        class="space-y-4"
      >
        <HaexSyncAddBackend
          ref="connectRef"
          v-model:server-url="credentials.serverUrl"
          v-model:email="credentials.email"
          v-model:password="credentials.password"
          :items="serverOptions"
          :is-loading="isLoading"
        />
      </div>

      <!-- Step 2: Select Vault -->
      <div
        v-else-if="currentStepIndex === 1"
        class="space-y-4"
      >
        <p class="text-sm text-base-content/60">
          {{ t('steps.selectVault.description') }}
        </p>

        <!-- Loading state -->
        <div
          v-if="isLoadingVaults"
          class="flex items-center justify-center p-8"
        >
          <span class="loading loading-spinner loading-lg"></span>
        </div>

        <!-- Vault list -->
        <div
          v-else-if="availableVaults.length > 0"
          class="space-y-2"
        >
          <div
            v-for="vault in availableVaults"
            :key="vault.vaultId"
            class="card bg-base-200 p-4 cursor-pointer hover:bg-base-300 transition-colors"
            :class="{
              'ring-2 ring-primary': selectedVaultId === vault.vaultId,
              'ring-2 ring-error': step2Error && !selectedVaultId,
            }"
            @click="selectedVaultId = vault.vaultId; step2Error = ''"
          >
            <div class="flex items-center justify-between">
              <div>
                <p class="font-medium">
                  {{
                    vault.decryptedName || t('steps.selectVault.encryptedVault')
                  }}
                </p>
                <p class="text-sm text-base-content/60">
                  {{ t('steps.selectVault.createdAt') }}:
                  {{ formatDate(vault.createdAt) }}
                </p>
              </div>
              <div
                v-if="selectedVaultId === vault.vaultId"
                class="text-primary"
              >
                <i class="i-lucide-check-circle text-2xl"></i>
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

        <!-- No vaults -->
        <div
          v-else
          class="text-center p-8 text-base-content/60"
        >
          <p>{{ t('steps.selectVault.noVaults') }}</p>
        </div>
      </div>

      <!-- Step 3: Enter Vault Password -->
      <div
        v-else-if="currentStepIndex === 2"
        class="space-y-4"
      >
        <p class="text-sm text-base-content/60">
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
            size="xl"
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
            :description="t('steps.enterVaultPassword.vaultPasswordDescription')"
            :schema="wizardSchema.vaultPassword"
            :check="check"
            leading-icon="i-lucide-lock"
            size="xl"
            class="w-full"
          />
        </div>
      </div>
    </div>

    <!-- Actions -->
    <div class="flex gap-2 mt-6">
      <UButton
        v-if="currentStepIndex > 0"
        color="neutral"
        variant="outline"
        @click="previousStep"
      >
        {{ t('actions.back') }}
      </UButton>
      <UButton
        v-if="showCancel && currentStepIndex === 0"
        color="neutral"
        variant="outline"
        @click="cancel"
      >
        {{ t('actions.cancel') }}
      </UButton>
      <div class="flex-1"></div>
      <UButton
        v-if="currentStepIndex < 2"
        color="primary"
        :disabled="!canProceed"
        :loading="isLoading"
        @click="nextStep"
      >
        {{ t('actions.next') }}
      </UButton>
      <UButton
        v-else
        color="primary"
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
  decryptStringAsync,
  deriveKeyFromPasswordAsync,
  base64ToArrayBuffer,
} from '~/utils/crypto/vaultKey'
import { createConnectWizardSchema } from './connectWizardSchema'

const { t } = useI18n()
const { serverOptions } = useSyncServerOptions()
const { add } = useToast()

// Create validation schema with i18n
const wizardSchema = computed(() => createConnectWizardSchema(t))

interface VaultInfo {
  vaultId: string
  encryptedVaultName: string
  vaultNameNonce: string
  vaultNameSalt: string
  createdAt: string
  decryptedName?: string
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
      email: string
      serverPassword: string
      vaultPassword: string
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

const connectRef = ref()
const isLoading = ref(false)
const check = ref(false)

// Step 1: Login
const credentials = ref({
  serverUrl: 'https://sync.haex.space',
  email: '',
  password: '',
})
const supabaseClient = ref<ReturnType<typeof createClient> | null>(null)
const step1Errors = reactive({
  serverUrl: [] as string[],
  email: [] as string[],
  password: [] as string[],
})

const isLoginFormValid = computed(() => {
  return (
    credentials.value.serverUrl !== '' &&
    credentials.value.email !== '' &&
    credentials.value.password !== '' &&
    step1Errors.serverUrl.length === 0 &&
    step1Errors.email.length === 0 &&
    step1Errors.password.length === 0
  )
})

// Step 2: Select Vault
const availableVaults = ref<VaultInfo[]>([])
const selectedVaultId = ref<string | null>(null)
const isLoadingVaults = ref(false)
const step2Error = ref('')

// Step 3: Enter Vault Password
const localVaultName = ref('')
const vaultNameExists = ref(false)
const vaultPassword = ref('')
const step3Errors = reactive({
  vaultName: [] as string[],
  password: [] as string[],
})

// Computed for step validation
const canProceed = computed(() => {
  if (currentStepIndex.value === 0) {
    return isLoginFormValid.value
  }
  if (currentStepIndex.value === 1) {
    return selectedVaultId.value !== null
  }
  return false
})

const isStep3Valid = computed(() => {
  return (
    localVaultName.value !== '' &&
    !vaultNameExists.value &&
    vaultPassword.value !== '' &&
    step3Errors.vaultName.length === 0 &&
    step3Errors.password.length === 0
  )
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
    // Validate Step 2 (vault selection)
    if (!selectedVaultId.value) {
      step2Error.value = t('errors.vaultSelectionRequired')
      return
    }

    // Pre-fill local vault name with the decrypted name from backend
    const selectedVault = availableVaults.value.find(
      (v) => v.vaultId === selectedVaultId.value,
    )
    if (selectedVault?.decryptedName) {
      localVaultName.value = selectedVault.decryptedName
      // Check if this name already exists locally
      await checkVaultNameExistsAsync()
    }

    currentStepIndex.value++
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

    // 2. Sign in via server-side endpoint (bypasses Turnstile captcha)
    const loginResponse = await fetch(`${credentials.value.serverUrl}/auth/login`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        email: credentials.value.email,
        password: credentials.value.password,
      }),
    })

    if (!loginResponse.ok) {
      const errorData = await loginResponse.json()
      throw new Error(errorData.error || 'Login failed')
    }

    const loginData = await loginResponse.json()

    // 3. Create Supabase client and set session from server response
    supabaseClient.value = createClient(
      serverInfo.supabaseUrl,
      serverInfo.supabaseAnonKey,
    )

    // Set the session from the server response
    await supabaseClient.value.auth.setSession({
      access_token: loginData.access_token,
      refresh_token: loginData.refresh_token,
    })

    // 4. Load available vaults
    await loadVaultsAsync()

    // 5. Move to next step
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

    // Decrypt vault names using server password and vaultNameSalt
    for (const vault of availableVaults.value) {
      try {
        const vaultNameSalt = base64ToArrayBuffer(vault.vaultNameSalt)
        const derivedKey = await deriveKeyFromPasswordAsync(
          credentials.value.password, // Server password
          vaultNameSalt,
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

  if (!selectedVaultId.value) return

  const selectedVault = availableVaults.value.find(
    (v) => v.vaultId === selectedVaultId.value,
  )
  if (!selectedVault) return

  // Emit complete event with all necessary data
  if (!supabaseClient.value) {
    throw new Error('Supabase client not initialized')
  }

  // Store Supabase client in syncEngineStore for later use
  const syncEngineStore = useSyncEngineStore()
  syncEngineStore.supabaseClient = supabaseClient.value

  emit('complete', {
    backendId: crypto.randomUUID(),
    vaultId: selectedVault.vaultId,
    vaultName: selectedVault.decryptedName || selectedVault.vaultId,
    localVaultName: localVaultName.value,
    serverUrl: credentials.value.serverUrl,
    email: credentials.value.email,
    serverPassword: credentials.value.password,
    vaultPassword: vaultPassword.value,
  })
}

const cancel = () => {
  emit('cancel')
}

const clearForm = () => {
  currentStepIndex.value = 0
  credentials.value = {
    serverUrl: 'https://sync.haex.space',
    email: '',
    password: '',
  }
  availableVaults.value = []
  selectedVaultId.value = null
  localVaultName.value = ''
  vaultPassword.value = ''
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
    selectVault:
      title: Vault auswählen
      description: Wähle einen Vault, den du synchronisieren möchtest
      encryptedVault: Verschlüsselter Vault
      createdAt: Erstellt am
      noVaults: Keine Vaults gefunden
    enterVaultPassword:
      title: Vault-Passwort
      description: Gib das Passwort deines Vaults ein, um die Synchronisierung einzurichten
      vaultName: Lokaler Vault-Name
      vaultNameDescription: Gib einen Namen für deinen lokalen Vault ein
      vaultNameExists: Ein Vault mit diesem Namen existiert bereits
      vaultPassword: Vault-Passwort
      vaultPasswordDescription: Das Passwort, mit dem du deinen Vault ursprünglich erstellt hast
  actions:
    login: Anmelden
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
    emailRequired: E-Mail ist erforderlich
    emailInvalid: Muss eine gültige E-Mail sein
    passwordRequired: Passwort ist erforderlich
    vaultNameRequired: Vault-Name ist erforderlich
    vaultNameTooLong: Vault-Name ist zu lang (max. 255 Zeichen)
    vaultPasswordMinLength: Passwort muss mindestens 6 Zeichen lang sein
    vaultPasswordTooLong: Passwort ist zu lang (max. 255 Zeichen)
en:
  steps:
    login:
      title: Connect
    selectVault:
      title: Select Vault
      description: Choose a vault you want to synchronize
      encryptedVault: Encrypted Vault
      createdAt: Created at
      noVaults: No vaults found
    enterVaultPassword:
      title: Vault Password
      description: Enter your vault password to set up synchronization
      vaultName: Local Vault Name
      vaultNameDescription: Enter a name for your local vault
      vaultNameExists: A vault with this name already exists
      vaultPassword: Vault Password
      vaultPasswordDescription: The password you used to originally create your vault
  actions:
    login: Login
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
    emailRequired: Email is required
    emailInvalid: Must be a valid email
    passwordRequired: Password is required
    vaultNameRequired: Vault name is required
    vaultNameTooLong: Vault name is too long (max. 255 characters)
    vaultPasswordMinLength: Password must be at least 6 characters
    vaultPasswordTooLong: Password is too long (max. 255 characters)
</i18n>
