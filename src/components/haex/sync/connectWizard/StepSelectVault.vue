<template>
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
        :key="vault.spaceId"
        class="card bg-elevated rounded-lg p-4 cursor-pointer hover:bg-muted transition-colors"
        :class="{
          'ring-2 ring-primary':
            selectedVaultId === vault.spaceId && !isCreatingNewVault,
          'ring-2 ring-error':
            step3Error && !selectedVaultId && !isCreatingNewVault,
        }"
        @click="emit('selectVault', vault.spaceId)"
      >
        <div class="flex items-center justify-between">
          <div>
            <p class="font-medium">
              {{
                decryptedVaultNames[vault.spaceId] ||
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
              selectedVaultId === vault.spaceId && !isCreatingNewVault
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
        @click="emit('selectNewVault')"
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
        v-model="localVaultNameModel"
        v-model:errors="step3Errors.vaultName"
        :label="t('steps.selectVault.vaultName')"
        :description="t('steps.selectVault.vaultNameDescription')"
        :schema="wizardSchema.vaultName"
        :check="check"
        class="w-full"
        @blur="emit('checkVaultName')"
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
          v-model="vaultPasswordModel"
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
          class="w-full"
        />

        <!-- Password confirmation for new vault -->
        <UiInputPassword
          v-if="isCreatingNewVault"
          v-model="vaultPasswordConfirmModel"
          v-model:errors="step3Errors.passwordConfirm"
          :label="t('steps.selectVault.confirmPassword')"
          :description="t('steps.selectVault.confirmPasswordDescription')"
          :schema="wizardSchema.vaultPassword"
          :check="check"
          leading-icon="i-lucide-lock"
          class="w-full"
        />
        <p
          v-if="
            isCreatingNewVault &&
            vaultPasswordConfirmModel &&
            vaultPasswordModel !== vaultPasswordConfirmModel
          "
          class="text-sm text-error -mt-3"
        >
          {{ t('steps.selectVault.passwordMismatch') }}
        </p>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import type {
  ConnectWizardStep3Errors,
  ConnectWizardVaultInfo,
} from '~/composables/useConnectWizard'
import type { createConnectWizardSchema } from '../connectWizardSchema'

const { t } = useI18n()

type WizardSchema = ReturnType<typeof createConnectWizardSchema>

defineProps<{
  availableVaults: ConnectWizardVaultInfo[]
  selectedVaultId: string | null
  isLoadingVaults: boolean
  step3Error: string
  isCreatingNewVault: boolean
  decryptedVaultNames: Record<string, string>
  needsVaultPassword: boolean
  isCheckingVaultPassword: boolean
  vaultPasswordVerified: boolean
  vaultNameExists: boolean
  step3Errors: ConnectWizardStep3Errors
  check: boolean
  wizardSchema: WizardSchema
}>()

const emit = defineEmits<{
  selectVault: [spaceId: string]
  selectNewVault: []
  checkVaultName: []
}>()

const localVaultNameModel = defineModel<string>('localVaultName', { required: true })
const vaultPasswordModel = defineModel<string>('vaultPassword', { required: true })
const vaultPasswordConfirmModel = defineModel<string>('vaultPasswordConfirm', { required: true })

const formatDate = (dateStr: string) => {
  return new Date(dateStr).toLocaleDateString()
}
</script>
