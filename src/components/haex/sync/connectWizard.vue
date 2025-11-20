<template>
  <div class="space-y-6">
    <!-- Stepper -->
    <UStepper
      v-model="currentStepIndex"
      :items="steps"
      class="mb-6"
    />

    <!-- Step Content -->
    <div class="min-h-[400px]">
      <!-- Step 1: Login -->
      <div v-if="currentStepIndex === 0" class="space-y-4">
        <HaexSyncConnect
          ref="connectRef"
          :is-loading="isLoading"
          @update="onCredentialsUpdate"
        />
      </div>

      <!-- Step 2: Select Vault -->
      <div v-else-if="currentStepIndex === 1" class="space-y-4">
        <p class="text-sm text-base-content/60">
          {{ t('steps.selectVault.description') }}
        </p>

        <!-- Loading state -->
        <div v-if="isLoadingVaults" class="flex items-center justify-center p-8">
          <span class="loading loading-spinner loading-lg"></span>
        </div>

        <!-- Vault list -->
        <div v-else-if="availableVaults.length > 0" class="space-y-2">
          <div
            v-for="vault in availableVaults"
            :key="vault.vaultId"
            class="card bg-base-200 p-4 cursor-pointer hover:bg-base-300 transition-colors"
            :class="{ 'ring-2 ring-primary': selectedVaultId === vault.vaultId }"
            @click="selectedVaultId = vault.vaultId"
          >
            <div class="flex items-center justify-between">
              <div>
                <p class="font-medium">{{ vault.decryptedName || t('steps.selectVault.encryptedVault') }}</p>
                <p class="text-sm text-base-content/60">
                  {{ t('steps.selectVault.createdAt') }}: {{ formatDate(vault.createdAt) }}
                </p>
              </div>
              <div v-if="selectedVaultId === vault.vaultId" class="text-primary">
                <i class="i-lucide-check-circle text-2xl"></i>
              </div>
            </div>
          </div>
        </div>

        <!-- No vaults -->
        <div v-else class="text-center p-8 text-base-content/60">
          <p>{{ t('steps.selectVault.noVaults') }}</p>
        </div>
      </div>

      <!-- Step 3: Create Local Vault -->
      <div v-else-if="currentStepIndex === 2" class="space-y-4">
        <p class="text-sm text-base-content/60">
          {{ t('steps.createVault.description') }}
        </p>

        <div class="space-y-4">
          <UFormField
            :label="t('steps.createVault.vaultName')"
            :description="t('steps.createVault.vaultNameDescription')"
          >
            <UiInput
              v-model="localVaultName"
              :placeholder="t('steps.createVault.vaultNamePlaceholder')"
              size="xl"
              class="w-full"
              @blur="checkVaultNameExistsAsync"
            />
          </UFormField>
          <p v-if="vaultNameExists" class="text-sm text-error mt-1">
            {{ t('steps.createVault.vaultNameExists') }}
          </p>

          <UFormField
            :label="t('steps.createVault.vaultPassword')"
            :description="t('steps.createVault.vaultPasswordDescription')"
          >
            <UiInputPassword
              v-model="newVaultPassword"
              :placeholder="t('steps.createVault.vaultPasswordPlaceholder')"
              size="xl"
              class="w-full"
            />
          </UFormField>

          <UFormField
            :label="t('steps.createVault.vaultPasswordConfirm')"
          >
            <UiInputPassword
              v-model="newVaultPasswordConfirm"
              :placeholder="t('steps.createVault.vaultPasswordConfirmPlaceholder')"
              size="xl"
              class="w-full"
            />
          </UFormField>
          <p v-if="newVaultPassword !== newVaultPasswordConfirm && newVaultPasswordConfirm !== ''" class="text-sm text-error mt-1">
            {{ t('steps.createVault.passwordMismatch') }}
          </p>
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
import { decryptStringAsync, deriveKeyFromPasswordAsync, base64ToArrayBuffer } from '~/utils/crypto/vaultKey'

const { t } = useI18n()
const { add } = useToast()

interface VaultInfo {
  vaultId: string
  encryptedVaultName: string
  vaultNameNonce: string
  salt: string
  createdAt: string
  decryptedName?: string
}

defineProps<{
  isLoading?: boolean
  showCancel?: boolean
}>()

const emit = defineEmits<{
  complete: [{
    backendId: string
    vaultId: string
    vaultName: string
    localVaultName: string
    serverUrl: string
    email: string
    password: string
    newVaultPassword?: string
  }]
  cancel: []
}>()

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
    label: t('steps.createVault.title'),
    icon: 'i-lucide-hard-drive',
  },
])

const connectRef = ref()
const isLoading = ref(false)

// Step 1: Login
const credentials = ref({
  serverUrl: '',
  email: '',
  password: '',
})
const supabaseClient = ref<ReturnType<typeof createClient> | null>(null)

const isLoginFormValid = computed(() => {
  return (
    credentials.value.serverUrl !== '' &&
    credentials.value.email !== '' &&
    credentials.value.password !== ''
  )
})

// Step 2: Select Vault
const availableVaults = ref<VaultInfo[]>([])
const selectedVaultId = ref<string | null>(null)
const isLoadingVaults = ref(false)

// Step 3: Create Local Vault
const localVaultName = ref('')
const vaultNameExists = ref(false)
const newVaultPassword = ref('')
const newVaultPasswordConfirm = ref('')

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
    newVaultPassword.value !== '' &&
    newVaultPassword.value === newVaultPasswordConfirm.value
  )
})

// Methods
const onCredentialsUpdate = (newCredentials: { serverUrl: string; email: string; password: string }) => {
  credentials.value = newCredentials
}

const nextStep = async () => {
  if (currentStepIndex.value === 0) {
    await loginAsync()
  } else if (currentStepIndex.value === 1) {
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
    supabaseClient.value = createClient(
      serverInfo.supabaseUrl,
      serverInfo.supabaseAnonKey,
    )

    // 2. Sign in
    const { error } = await supabaseClient.value.auth.signInWithPassword({
      email: credentials.value.email,
      password: credentials.value.password,
    })

    if (error) {
      throw new Error(error.message)
    }

    add({
      title: t('success.loggedIn'),
      color: 'success',
    })

    // 3. Load available vaults
    await loadVaultsAsync()

    // 4. Move to next step
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
    const { data: { session } } = await supabaseClient.value.auth.getSession()
    if (!session?.access_token) {
      throw new Error('Not authenticated')
    }

    // Fetch vaults from server
    const response = await fetch(`${credentials.value.serverUrl}/sync/vaults`, {
      method: 'GET',
      headers: {
        'Authorization': `Bearer ${session.access_token}`,
      },
    })

    if (!response.ok) {
      throw new Error('Failed to fetch vaults')
    }

    const data = await response.json()
    availableVaults.value = data.vaults

    // Try to decrypt vault names
    for (const vault of availableVaults.value) {
      try {
        const salt = base64ToArrayBuffer(vault.salt)
        const derivedKey = await deriveKeyFromPasswordAsync(credentials.value.password, salt)
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
  if (!selectedVaultId.value) return

  const selectedVault = availableVaults.value.find(v => v.vaultId === selectedVaultId.value)
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
    password: credentials.value.password,
    newVaultPassword: newVaultPassword.value,
  })
}

const cancel = () => {
  emit('cancel')
}

const clearForm = () => {
  currentStepIndex.value = 0
  credentials.value = {
    serverUrl: '',
    email: '',
    password: '',
  }
  availableVaults.value = []
  selectedVaultId.value = null
  localVaultName.value = ''
  newVaultPassword.value = ''
  newVaultPasswordConfirm.value = ''
  vaultNameExists.value = false
  supabaseClient.value = null
  connectRef.value?.clearForm()
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
    createVault:
      title: Lokaler Vault
      description: Erstelle einen neuen lokalen Vault und synchronisiere ihn mit dem ausgewählten Server-Vault
      vaultName: Vault-Name
      vaultNameDescription: Gib einen eindeutigen Namen für deinen lokalen Vault ein
      vaultNamePlaceholder: Mein Vault
      vaultNameExists: Ein Vault mit diesem Namen existiert bereits
      vaultPassword: Vault-Passwort
      vaultPasswordDescription: Wähle ein sicheres Passwort für deinen Vault
      vaultPasswordPlaceholder: Passwort eingeben
      vaultPasswordConfirm: Passwort bestätigen
      vaultPasswordConfirmPlaceholder: Passwort erneut eingeben
      passwordMismatch: Passwörter stimmen nicht überein
  actions:
    login: Anmelden
    back: Zurück
    next: Weiter
    complete: Abschließen
    cancel: Abbrechen
  success:
    loggedIn: Erfolgreich angemeldet
  errors:
    serverConnection: Verbindung zum Server fehlgeschlagen
    loginFailed: Anmeldung fehlgeschlagen
    loadVaultsFailed: Vaults konnten nicht geladen werden
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
    createVault:
      title: Local Vault
      description: Create a new local vault and sync it with the selected server vault
      vaultName: Vault Name
      vaultNameDescription: Enter a unique name for your local vault
      vaultNamePlaceholder: My Vault
      vaultNameExists: A vault with this name already exists
      vaultPassword: Vault Password
      vaultPasswordDescription: Choose a secure password for your vault
      vaultPasswordPlaceholder: Enter password
      vaultPasswordConfirm: Confirm Password
      vaultPasswordConfirmPlaceholder: Re-enter password
      passwordMismatch: Passwords do not match
  actions:
    login: Login
    back: Back
    next: Next
    complete: Complete
    cancel: Cancel
  success:
    loggedIn: Successfully logged in
  errors:
    serverConnection: Failed to connect to server
    loginFailed: Login failed
    loadVaultsFailed: Failed to load vaults
</i18n>
