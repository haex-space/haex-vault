<template>
  <UiDrawer
    v-if="isSmallScreen"
    v-model:open="open"
    :title="t('title')"
    :description="t('description')"
  >
    <UiButton
      :label="t('button.label')"
      :ui="{
        base: 'px-4 py-3',
      }"
      icon="mdi:plus"
      size="xl"
      variant="outline"
      block
    />

    <template #header>
      <h2 class="text-xl font-semibold">
        {{ t('title') }}
      </h2>
    </template>

    <template #body>
      <UForm
        :state="vault"
        class="w-full space-y-6"
        @keyup.enter="onCreateAsync"
      >
        <UiInput
          v-model="vault.name"
          v-model:errors="errors.name"
          icon="mdi:safe"
          :label="t('vault.placeholder')"
          :schema="vaultSchema.name"
          :check="check"
          :custom-validators="[checkVaultNameExists]"
          autofocus
          size="xl"
          class="w-full"
        />

        <UiInputPassword
          v-model="vault.password"
          v-model:errors="errors.password"
          :label="t('password.placeholder')"
          :schema="vaultSchema.password"
          :check="check"
          leading-icon="i-lucide-lock"
          size="xl"
          class="w-full"
        />

        <UiInputPassword
          v-model="vault.passwordConfirm"
          v-model:errors="errors.passwordConfirm"
          :label="t('passwordConfirm.placeholder')"
          :check="check"
          leading-icon="i-lucide-lock"
          size="xl"
          class="w-full"
        />
      </UForm>
    </template>

    <template #footer>
      <div class="flex gap-3">
        <UButton
          color="neutral"
          variant="outline"
          block
          size="xl"
          @click="open = false"
        >
          {{ t('cancel') }}
        </UButton>
        <UButton
          color="primary"
          block
          size="xl"
          @click="onCreateAsync"
        >
          {{ t('create') }}
        </UButton>
      </div>
    </template>
  </UiDrawer>

  <UModal
    v-else
    v-model:open="open"
    :title="t('title')"
    :description="t('description')"
  >
    <UiButton
      :label="t('button.label')"
      :ui="{
        base: 'px-4 py-3 ',
      }"
      icon="mdi:plus"
      size="xl"
      variant="outline"
      block
    />

    <template #body>
      <div class="space-y-4">
        <UForm
          :state="vault"
          class="w-full space-y-6"
          @keyup.enter="onCreateAsync"
        >
          <UiInput
            v-model="vault.name"
            v-model:errors="errors.name"
            icon="mdi:safe"
            :label="t('vault.label')"
            :schema="vaultSchema.name"
            :check="check"
            :custom-validators="[checkVaultNameExists]"
            autofocus
            size="xl"
            class="w-full"
          />

          <UiInputPassword
            v-model="vault.password"
            v-model:errors="errors.password"
            :label="t('password.placeholder')"
            :schema="vaultSchema.password"
            :check="check"
            leading-icon="i-lucide-lock"
            size="xl"
            class="w-full"
          />

          <UiInputPassword
            v-model="vault.passwordConfirm"
            v-model:errors="errors.passwordConfirm"
            :label="t('passwordConfirm.label')"
            :check="check"
            leading-icon="i-lucide-lock"
            size="xl"
            class="w-full"
          />
        </UForm>
      </div>
    </template>

    <template #footer>
      <div class="flex gap-3 w-full">
        <UButton
          color="neutral"
          variant="outline"
          block
          size="xl"
          @click="open = false"
        >
          {{ t('cancel') }}
        </UButton>
        <UButton
          color="primary"
          block
          size="xl"
          @click="onCreateAsync"
        >
          {{ t('create') }}
        </UButton>
      </div>
    </template>
  </UModal>
</template>

<script setup lang="ts">
import type { AcceptableValue } from '@nuxt/ui/runtime/types/utils.js'
import { vaultSchema } from './schema'

const open = defineModel<boolean>('open', { default: false })
const { isSmallScreen } = storeToRefs(useUiStore())

const { t } = useI18n({
  useScope: 'local',
})

const vault = reactive({
  name: 'HaexVault',
  password: '',
  passwordConfirm: '',
  type: 'password' as 'password' | 'text',
})

const errors = reactive<{
  name: string[]
  password: string[]
  passwordConfirm: string[]
}>({
  name: [],
  password: [],
  passwordConfirm: [],
})

const initVault = () => {
  vault.name = 'HaexVault'
  vault.password = ''
  vault.passwordConfirm = ''
  vault.type = 'password'
}

const clearErrors = () => {
  errors.name = []
  errors.password = []
  errors.passwordConfirm = []
}
const { createAsync } = useVaultStore()
const { lastVaults } = storeToRefs(useLastVaultStore())
const { add } = useToast()

const check = ref(false)

// Custom validator to check if vault name already exists
const checkVaultNameExists = (
  vaultName: AcceptableValue | undefined,
): string | null => {
  if (!vaultName || typeof vaultName !== 'string') return null

  const inputName = vaultName.toLowerCase()
  const vaultNameExists = lastVaults.value.some((v) => {
    const existingName = v.name.toLowerCase()
    const existingNameWithoutExt = existingName.replace(/\.db$/, '')
    return existingName === inputName || existingNameWithoutExt === inputName
  })

  return vaultNameExists ? t('error.vaultExists.description') : null
}

const onCreateAsync = async () => {
  // Trigger validation in all input fields
  check.value = true

  // Validate password confirmation manually (no schema for this)
  if (vault.password !== vault.passwordConfirm) {
    errors.passwordConfirm = [t('error.passwordMismatch.description')]
  } else {
    errors.passwordConfirm = []
  }

  // Wait for validation to complete
  await nextTick()

  // If there are any errors, don't proceed
  if (
    errors.name.length > 0 ||
    errors.password.length > 0 ||
    errors.passwordConfirm.length > 0
  ) {
    return
  }

  open.value = false
  try {
    if (vault.name && vault.password) {
      const vaultId = await createAsync({
        vaultName: vault.name,
        password: vault.password,
      })

      if (vaultId) {
        initVault()
        clearErrors()
        check.value = false
        await navigateTo(
          useLocaleRoute()({ name: 'desktop', params: { vaultId } }),
        )
      }
    }
  } catch (error) {
    console.error(error)
    add({ color: 'error', description: JSON.stringify(error) })
  }
}
</script>

<i18n lang="yaml">
de:
  button:
    label: Vault erstellen
  vault:
    label: Vaultname
    placeholder: Vaultname
  password:
    label: Passwort
    placeholder: Passwort eingeben
  passwordConfirm:
    label: Passwort bestätigen
    placeholder: Passwort wiederholen
  title: Neue HaexVault erstellen
  create: Erstellen
  cancel: Abbrechen
  description: Erstelle eine neue Vault für deine Daten
  error:
    passwordMismatch:
      title: Passwörter stimmen nicht überein
      description: Bitte stelle sicher, dass beide Passwörter identisch sind
    vaultExists:
      title: Vault existiert bereits
      description: Eine Vault mit diesem Namen existiert bereits
    validation:
      title: Validierungsfehler
      name: Bitte gib einen gültigen Vaultnamen ein
      password: Das Passwort muss mindestens 6 Zeichen lang sein

en:
  button:
    label: Create vault
  vault:
    label: Vault name
    placeholder: Vault name
  password:
    label: Password
    placeholder: Enter password
  passwordConfirm:
    label: Confirm password
    placeholder: Repeat password
  title: Create new HaexVault
  create: Create
  cancel: Cancel
  description: Create a new vault for your data
  error:
    passwordMismatch:
      title: Passwords do not match
      description: Please make sure both passwords are identical
    vaultExists:
      title: Vault already exists
      description: A vault with this name already exists
    validation:
      title: Validation error
      name: Please enter a valid vault name
      password: Password must be at least 6 characters long
</i18n>
