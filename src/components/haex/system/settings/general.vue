<template>
  <div>
    <div class="p-6 border-b border-base-content/10">
      <h2 class="text-2xl font-bold">
        {{ t('title') }}
      </h2>
    </div>

    <div class="p-6 space-y-6">
      <UFormField :label="t('language')" :description="t('language.description')">
        <UiDropdownLocale @select="onSelectLocaleAsync" />
      </UFormField>

      <UFormField
        :label="t('vaultName.label')"
        :description="t('vaultName.description')"
      >
        <UiInput
          v-model="currentVaultName"
          :placeholder="t('vaultName.label')"
          @change="onSetVaultNameAsync"
        />
      </UFormField>

      <UFormField
        :label="t('deviceName.label')"
        :description="t('deviceName.description')"
      >
        <UiInput
          v-model="deviceName"
          :placeholder="t('deviceName.label')"
          @change="onUpdateDeviceNameAsync"
        />
      </UFormField>

      <!-- Passwort ändern Section -->
      <USeparator class="my-6" />

      <UFormField
        :label="t('password.label')"
        :description="t('password.description')"
      >
        <UiDrawerModal
          v-model:open="isPasswordModalOpen"
          :title="t('password.modal.title')"
        >
          <template #trigger>
            <UButton
              color="neutral"
              variant="outline"
              :label="t('password.button')"
              icon="i-heroicons-key"
            />
          </template>

          <template #content>
            <form
              class="space-y-4 pt-2"
              @submit.prevent="onChangePasswordAsync"
            >
              <UiInputPassword
                v-model="passwordForm.currentPassword"
                v-model:errors="currentPasswordErrors"
                :label="t('password.modal.currentPassword')"
              />

              <UiInputPassword
                v-model="passwordForm.newPassword"
                v-model:errors="newPasswordErrors"
                :label="t('password.modal.newPassword')"
              />

              <UiInputPassword
                v-model="passwordForm.confirmPassword"
                v-model:errors="confirmPasswordErrors"
                :label="t('password.modal.confirmPassword')"
              />
            </form>
          </template>

          <template #footer>
            <div class="flex justify-end gap-2 w-full">
              <UButton
                color="neutral"
                variant="ghost"
                :label="t('password.modal.cancel')"
                @click="isPasswordModalOpen = false"
              />
              <UButton
                color="primary"
                :label="t('password.modal.submit')"
                :loading="isChangingPassword"
                @click="onChangePasswordAsync"
              />
            </div>
          </template>
        </UiDrawerModal>
      </UFormField>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { Locale } from 'vue-i18n'
import { createChangePasswordSchema } from '~/components/haex/vault/schema'

const { t, setLocale } = useI18n()
const { add } = useToast()

const { currentVaultName } = storeToRefs(useVaultStore())
const { changePasswordAsync } = useVaultStore()
const { updateVaultNameAsync, updateLocaleAsync } = useVaultSettingsStore()

const { deviceName } = storeToRefs(useDeviceStore())
const { updateDeviceNameAsync, readDeviceNameAsync } = useDeviceStore()

// Password change state
const isPasswordModalOpen = ref(false)
const isChangingPassword = ref(false)
const passwordForm = reactive({
  currentPassword: '',
  newPassword: '',
  confirmPassword: '',
})
const currentPasswordErrors = ref<string[]>([])
const newPasswordErrors = ref<string[]>([])
const confirmPasswordErrors = ref<string[]>([])

const resetPasswordForm = () => {
  passwordForm.currentPassword = ''
  passwordForm.newPassword = ''
  passwordForm.confirmPassword = ''
  currentPasswordErrors.value = []
  newPasswordErrors.value = []
  confirmPasswordErrors.value = []
}

const validatePasswordForm = (): boolean => {
  currentPasswordErrors.value = []
  newPasswordErrors.value = []
  confirmPasswordErrors.value = []

  const schema = createChangePasswordSchema(t)
  const result = schema.safeParse(passwordForm)

  if (!result.success) {
    for (const error of result.error.errors) {
      const field = error.path[0] as string
      if (field === 'currentPassword') {
        currentPasswordErrors.value.push(error.message)
      } else if (field === 'newPassword') {
        newPasswordErrors.value.push(error.message)
      } else if (field === 'confirmPassword') {
        confirmPasswordErrors.value.push(error.message)
      }
    }
    return false
  }

  return true
}

const onChangePasswordAsync = async () => {
  if (!validatePasswordForm()) return

  isChangingPassword.value = true

  try {
    const result = await changePasswordAsync(
      passwordForm.currentPassword,
      passwordForm.newPassword,
    )

    if (result.success) {
      add({
        title: t('password.success.title'),
        description: t('password.success.description'),
        color: 'success',
      })
      isPasswordModalOpen.value = false
      resetPasswordForm()
    } else {
      if (result.error === 'Current password is incorrect') {
        currentPasswordErrors.value = [t('password.errors.incorrect')]
      } else {
        add({
          title: t('password.error.title'),
          description: result.error || t('password.error.description'),
          color: 'error',
        })
      }
    }
  } catch (error) {
    console.error('Password change error:', error)
    add({
      title: t('password.error.title'),
      description: t('password.error.description'),
      color: 'error',
    })
  } finally {
    isChangingPassword.value = false
  }
}

// Reset form when modal closes
watch(isPasswordModalOpen, (open) => {
  if (!open) {
    resetPasswordForm()
  }
})

const onSelectLocaleAsync = async (locale: Locale) => {
  await updateLocaleAsync(locale)
  await setLocale(locale)
}

const onSetVaultNameAsync = async () => {
  try {
    await updateVaultNameAsync(currentVaultName.value)
    add({ description: t('vaultName.update.success'), color: 'success' })
  } catch (error) {
    console.error(error)
    add({ description: t('vaultName.update.error'), color: 'error' })
  }
}

const onUpdateDeviceNameAsync = async () => {
  const check = vaultDeviceNameSchema.safeParse(deviceName.value)
  if (!check.success) return
  try {
    await updateDeviceNameAsync({ name: deviceName.value })
    add({ description: t('deviceName.update.success'), color: 'success' })
  } catch (error) {
    console.log(error)
    add({ description: t('deviceName.update.error'), color: 'error' })
  }
}

onMounted(async () => {
  await readDeviceNameAsync()
})
</script>

<i18n lang="yaml">
de:
  title: Allgemein
  language: Sprache
  language.description: Wähle deine bevorzugte Sprache
  vaultName:
    label: Vaultname
    description: Der Name deiner Vault
    update:
      success: Vaultname erfolgreich aktualisiert
      error: Vaultname konnte nicht aktualisiert werden
  deviceName:
    label: Gerätename
    description: Ein Name für dieses Gerät zur besseren Identifikation
    update:
      success: Gerätename wurde erfolgreich aktualisiert
      error: Gerätename konnte nich aktualisiert werden
  password:
    label: Vault-Passwort
    description: Ändere das Passwort für deine Vault
    button: Passwort ändern
    modal:
      title: Vault-Passwort ändern
      currentPassword: Aktuelles Passwort
      newPassword: Neues Passwort
      confirmPassword: Passwort bestätigen
      cancel: Abbrechen
      submit: Passwort ändern
    errors:
      currentRequired: Aktuelles Passwort ist erforderlich
      newRequired: Neues Passwort ist erforderlich
      confirmRequired: Passwortbestätigung ist erforderlich
      minLength: Passwort muss mindestens 6 Zeichen lang sein
      mismatch: Passwörter stimmen nicht überein
      incorrect: Aktuelles Passwort ist falsch
    success:
      title: Passwort geändert
      description: Dein Vault-Passwort wurde erfolgreich geändert
    error:
      title: Fehler
      description: Passwort konnte nicht geändert werden
en:
  title: General
  language: Language
  language.description: Choose your preferred language
  vaultName:
    label: Vault Name
    description: The name of your vault
    update:
      success: Vault Name successfully updated
      error: Vault name could not be updated
  deviceName:
    label: Device name
    description: A name for this device for better identification
    update:
      success: Device name has been successfully updated
      error: Device name could not be updated
  password:
    label: Vault Password
    description: Change the password for your vault
    button: Change Password
    modal:
      title: Change Vault Password
      currentPassword: Current Password
      newPassword: New Password
      confirmPassword: Confirm Password
      cancel: Cancel
      submit: Change Password
    errors:
      currentRequired: Current password is required
      newRequired: New password is required
      confirmRequired: Password confirmation is required
      minLength: Password must be at least 6 characters
      mismatch: Passwords do not match
      incorrect: Current password is incorrect
    success:
      title: Password Changed
      description: Your vault password has been changed successfully
    error:
      title: Error
      description: Password could not be changed
</i18n>
