import type { RecoveryKeyData } from '~/composables/useIdentityRecovery'

export interface ConnectWizardVaultInfo {
  spaceId: string
  encryptedVaultName: string
  vaultNameNonce: string
  vaultNameSalt: string
  ephemeralPublicKey: string
  createdAt: string
}

export interface ConnectWizardCredentials {
  originUrl: string
  identityId: string
}

export interface ConnectWizardStep3Errors {
  vaultName: string[]
  password: string[]
  passwordConfirm: string[]
}

/**
 * State and step navigation for the Sync Connect wizard.
 *
 * Keeps step index + all per-step reactive state alongside derived
 * `canProceed` / `canComplete` flags, so the parent + step components
 * can stay presentational.
 */
export function useConnectWizard() {
  // Stepper state
  const currentStepIndex = ref(0)

  // OTP step data (passed from email step)
  const otpServerUrl = ref('')
  const otpEmail = ref('')

  // Step 1: Identity Auth (via Recovery)
  const credentials = ref<ConnectWizardCredentials>({
    originUrl: 'https://sync.haex.space',
    identityId: '',
  })
  // Recovery mode: stores encrypted private key data from OTP verification
  const recoveredKeyData = ref<RecoveryKeyData | null>(null)

  // Step 2: DID Password (decrypt identity private key)
  const didPassword = ref('')
  const didPasswordError = ref('')
  const decryptedPrivateKey = ref<string | null>(null)

  // Step 3: Select Vault + optional vault password
  const availableVaults = ref<ConnectWizardVaultInfo[]>([])
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
  const step3Errors = reactive<ConnectWizardStep3Errors>({
    vaultName: [],
    password: [],
    passwordConfirm: [],
  })

  const isLoading = ref(false)
  const check = ref(false)

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

  const previousStep = () => {
    currentStepIndex.value--
  }

  const reset = () => {
    currentStepIndex.value = 0
    otpServerUrl.value = ''
    otpEmail.value = ''
    credentials.value = {
      originUrl: 'https://sync.haex.space',
      identityId: '',
    }
    availableVaults.value = []
    selectedVaultId.value = null
    isLoadingVaults.value = false
    step3Error.value = ''
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
    step3Errors.vaultName = []
    step3Errors.password = []
    step3Errors.passwordConfirm = []
    isLoading.value = false
    check.value = false
  }

  return {
    // Step index
    currentStepIndex,
    previousStep,
    // OTP
    otpServerUrl,
    otpEmail,
    // Identity
    credentials,
    recoveredKeyData,
    // Step 2
    didPassword,
    didPasswordError,
    decryptedPrivateKey,
    // Step 3
    availableVaults,
    selectedVaultId,
    isLoadingVaults,
    step3Error,
    isCreatingNewVault,
    decryptedVaultNames,
    needsVaultPassword,
    isCheckingVaultPassword,
    vaultPasswordVerified,
    localVaultName,
    vaultNameExists,
    vaultPassword,
    vaultPasswordConfirm,
    step3Errors,
    // Shared
    isLoading,
    check,
    // Derived
    canProceed,
    canComplete,
    // Lifecycle
    reset,
  }
}
