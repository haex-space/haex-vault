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

    <template #content>
      <div class="p-6 flex flex-col">
        <div class="w-full mx-auto space-y-4">
          <h2 class="text-xl font-semibold">
            {{ t('title') }}
          </h2>

          <UForm
            :state="vault"
            class="w-full space-y-6"
          >
            <UiInput
              v-model="vault.name"
              icon="mdi:safe"
              :label="t('vault.placeholder')"
              autofocus
              size="xl"
              class="w-full"
            />

            <UiInputPassword
              v-model="vault.password"
              :label="t('password.placeholder')"
              leading-icon="i-heroicons-key"
              size="xl"
              class="w-full"
            />

            <UiInputPassword
              v-model="vault.passwordConfirm"
              :label="t('passwordConfirm.placeholder')"
              leading-icon="i-heroicons-key"
              size="xl"
              class="w-full"
            />
          </UForm>
        </div>

        <div class="flex gap-3 mt-12">
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
        base: 'px-3 py-2',
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
        >
          <UFormField
            :label="t('vault.label')"
            name="name"
          >
            <UInput
              v-model="vault.name"
              icon="mdi:safe"
              :placeholder="t('vault.placeholder')"
              autofocus
              size="xl"
              class="w-full"
            />
          </UFormField>

          <UFormField
            :label="t('password.label')"
            name="password"
          >
            <UiInputPassword
              v-model="vault.password"
              :label="t('password.placeholder')"
              leading-icon="i-heroicons-key"
              size="xl"
              class="w-full"
            />
          </UFormField>

          <UFormField
            :label="t('passwordConfirm.label')"
            name="passwordConfirm"
          >
            <UiInputPassword
              v-model="vault.passwordConfirm"
              :label="t('passwordConfirm.placeholder')"
              leading-icon="i-heroicons-key"
              size="xl"
              class="w-full"
            />
          </UFormField>
        </UForm>
      </div>
    </template>

    <template #footer>
      <div class="flex gap-3 w-full">
        <UButton
          color="neutral"
          variant="outline"
          block
          @click="open = false"
        >
          {{ t('cancel') }}
        </UButton>
        <UButton
          color="primary"
          block
          @click="onCreateAsync"
        >
          {{ t('create') }}
        </UButton>
      </div>
    </template>
  </UModal>
</template>

<script setup lang="ts">
import { vaultSchema } from './schema'

const open = defineModel<boolean>('open', { default: false })
const { isSmallScreen } = storeToRefs(useUiStore())

const { t } = useI18n({
  useScope: 'local',
})

const vault = reactive<{
  name: string
  password: string
  passwordConfirm: string
  type: 'password' | 'text'
}>({
  name: 'HaexVault',
  password: '',
  passwordConfirm: '',
  type: 'password',
})

const initVault = () => {
  vault.name = 'HaexVault'
  vault.password = ''
  vault.passwordConfirm = ''
  vault.type = 'password'
}

const { createAsync } = useVaultStore()
const { add } = useToast()

const check = ref(false)

const onCreateAsync = async () => {
  check.value = true

  const nameCheck = vaultSchema.name.safeParse(vault.name)
  const passwordCheck = vaultSchema.password.safeParse(vault.password)

  if (!nameCheck.success) {
    add({
      color: 'error',
      title: t('error.validation.title'),
      description:
        nameCheck.error.errors[0]?.message || t('error.validation.name'),
    })
    return
  }

  if (!passwordCheck.success) {
    add({
      color: 'error',
      title: t('error.validation.title'),
      description:
        passwordCheck.error.errors[0]?.message ||
        t('error.validation.password'),
    })
    return
  }

  if (vault.password !== vault.passwordConfirm) {
    add({
      color: 'error',
      title: t('error.passwordMismatch.title'),
      description: t('error.passwordMismatch.description'),
    })
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
    validation:
      title: Validation error
      name: Please enter a valid vault name
      password: Password must be at least 6 characters long
</i18n>
