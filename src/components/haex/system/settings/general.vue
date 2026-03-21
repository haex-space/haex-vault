<template>
  <HaexSystemSettingsLayout :title="t('title')">
    <!-- General Settings Card -->
    <UCard>
      <template #header>
        <h3 class="text-lg font-semibold">{{ t('general.title') }}</h3>
      </template>

      <div class="space-y-4">
        <UFormField :label="t('language')" :description="t('language.description')">
          <UiSelectLocale @select="onSelectLocaleAsync" />
        </UFormField>

        <UFormField :label="t('vaultName.label')" :description="t('vaultName.description')">
          <UiInput
            v-model="currentVaultName"
            :placeholder="t('vaultName.label')"
            @change="onSetVaultNameAsync"
          />
        </UFormField>

        <UFormField :label="t('notifications.label')" :description="t('notifications.description')">
          <UiButton
            :label="isNotificationAllowed ? t('notifications.granted') : t('notifications.requestPermission')"
            :icon="isNotificationAllowed ? 'i-heroicons-check-circle' : 'i-heroicons-bell'"
            :color="isNotificationAllowed ? 'success' : 'primary'"
            :disabled="isNotificationAllowed"
            @click="requestNotificationPermissionAsync"
          />
        </UFormField>

        <UFormField :label="t('iconSize.label')" :description="t('iconSize.description')">
          <USelect
            v-model="iconSizePreset"
            :items="iconSizePresetOptions"
            class="w-40"
          />
        </UFormField>

        <USeparator />

        <UFormField :label="t('password.label')" :description="t('password.description')">
          <UiDrawerModal v-model:open="isPasswordModalOpen" :title="t('password.modal.title')">
            <template #trigger>
              <UiButton
                color="neutral"
                variant="outline"
                :label="t('password.button')"
                icon="i-heroicons-key"
              />
            </template>
            <template #content>
              <form class="space-y-4 pt-2" @submit.prevent="onChangePasswordAsync">
                <UiInputPassword v-model="passwordForm.currentPassword" v-model:errors="currentPasswordErrors" :label="t('password.modal.currentPassword')" />
                <UiInputPassword v-model="passwordForm.newPassword" v-model:errors="newPasswordErrors" :label="t('password.modal.newPassword')" />
                <UiInputPassword v-model="passwordForm.confirmPassword" v-model:errors="confirmPasswordErrors" :label="t('password.modal.confirmPassword')" />
              </form>
            </template>
            <template #footer>
              <div class="flex justify-end gap-2 w-full">
                <UiButton color="neutral" variant="ghost" :label="t('password.modal.cancel')" @click="isPasswordModalOpen = false" />
                <UiButton color="primary" :label="t('password.modal.submit')" :loading="isChangingPassword" @click="onChangePasswordAsync" />
              </div>
            </template>
          </UiDrawerModal>
        </UFormField>
      </div>
    </UCard>

    <!-- Appearance Card -->
    <UCard>
      <template #header>
        <h3 class="text-lg font-semibold">{{ t('appearance.title') }}</h3>
      </template>

      <div class="space-y-4">
        <UFormField :label="t('appearance.design.label')" :description="t('appearance.design.description')">
          <UiSelectTheme @select="onSelectThemeAsync" />
        </UFormField>

        <UFormField :label="t('appearance.workspaceBackground.label')" :description="t('appearance.workspaceBackground.description')">
          <div class="flex gap-2">
            <UiButton :label="t('appearance.workspaceBackground.choose')" variant="outline" color="neutral" @click="selectBackgroundImage" />
            <UiButton v-if="currentWorkspace?.background" :label="t('appearance.workspaceBackground.remove.label')" color="error" @click="removeBackgroundImage" />
          </div>
        </UFormField>

        <UFormField :label="t('appearance.gradient.variant.label')" :description="t('appearance.gradient.variant.description')">
          <USelect v-model="gradientVariant" :items="gradientVariantOptions" />
        </UFormField>

        <UFormField :label="t('appearance.gradient.enabled.label')" :description="t('appearance.gradient.enabled.description')">
          <UiToggle v-model="gradientEnabled" @update:model-value="onToggleGradientAsync" />
        </UFormField>
      </div>
    </UCard>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import type { Locale } from 'vue-i18n'
import { createChangePasswordSchema } from '~/components/haex/vault/schema'
import { DesktopIconSizePreset } from '~/stores/vault/settings'
import { open } from '@tauri-apps/plugin-dialog'
import { readFile, writeFile, mkdir, exists, remove } from '@tauri-apps/plugin-fs'
import { appLocalDataDir } from '@tauri-apps/api/path'

const { t, setLocale } = useI18n()
const { add } = useToast()

// General
const { currentVaultName } = storeToRefs(useVaultStore())
const { changePasswordAsync } = useVaultStore()
const { updateVaultNameAsync, updateLocaleAsync } = useVaultSettingsStore()

const { isNotificationAllowed } = storeToRefs(useNotificationStore())
const { requestNotificationPermissionAsync, checkNotificationAsync } = useNotificationStore()

const desktopStore = useDesktopStore()
const { iconSizePreset } = storeToRefs(desktopStore)
const { updateDesktopIconSizeAsync } = desktopStore

const iconSizePresetOptions = computed(() => [
  { label: t('iconSize.presets.small'), value: DesktopIconSizePreset.small },
  { label: t('iconSize.presets.medium'), value: DesktopIconSizePreset.medium },
  { label: t('iconSize.presets.large'), value: DesktopIconSizePreset.large },
  { label: t('iconSize.presets.extraLarge'), value: DesktopIconSizePreset.extraLarge },
])

watch(iconSizePreset, async (newPreset) => {
  if (newPreset) await updateDesktopIconSizeAsync(newPreset)
})

// Appearance
const { currentThemeName } = storeToRefs(useUiStore())
const { updateThemeAsync } = useVaultSettingsStore()

const workspaceStore = useWorkspaceStore()
const { currentWorkspace } = storeToRefs(workspaceStore)
const { updateWorkspaceBackgroundAsync } = workspaceStore

const gradientStore = useGradientStore()
const { gradientVariant, gradientEnabled } = storeToRefs(gradientStore)
const { syncGradientVariantAsync, syncGradientEnabledAsync, setGradientVariantAsync, toggleGradientAsync } = gradientStore

const gradientVariantOptions = [
  { label: t('appearance.gradient.variant.options.gitlab'), value: 'gitlab' },
  { label: t('appearance.gradient.variant.options.ocean'), value: 'ocean' },
  { label: t('appearance.gradient.variant.options.sunset'), value: 'sunset' },
  { label: t('appearance.gradient.variant.options.forest'), value: 'forest' },
]

const onSelectThemeAsync = async (theme: string) => {
  currentThemeName.value = theme
  await updateThemeAsync(theme)
}

watch(gradientVariant, async (newVariant) => {
  if (newVariant) await setGradientVariantAsync(newVariant)
})

const onToggleGradientAsync = async (enabled: boolean) => {
  try {
    await toggleGradientAsync(enabled)
    add({ description: t('appearance.gradient.enabled.success'), color: 'success' })
  } catch (error) {
    console.error(error)
    add({ description: t('appearance.gradient.enabled.error'), color: 'error' })
  }
}

const selectBackgroundImage = async () => {
  if (!currentWorkspace.value) return
  try {
    const selected = await open({
      multiple: false,
      filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp'] }],
    })
    if (!selected || typeof selected !== 'string') return

    const fileData = await readFile(selected)

    let ext = 'jpg'
    if (fileData.length > 4) {
      if (fileData[0] === 0x89 && fileData[1] === 0x50 && fileData[2] === 0x4e && fileData[3] === 0x47) ext = 'png'
      else if (fileData[0] === 0xff && fileData[1] === 0xd8 && fileData[2] === 0xff) ext = 'jpg'
      else if (fileData[0] === 0x52 && fileData[1] === 0x49 && fileData[2] === 0x46 && fileData[3] === 0x46) ext = 'webp'
    }

    const appDataPath = await appLocalDataDir()
    const fileName = `workspace-${currentWorkspace.value.id}-background.${ext}`
    const targetPath = `${appDataPath}/files/${fileName}`
    const parentDir = `${appDataPath}/files`

    if (!(await exists(parentDir))) await mkdir(parentDir, { recursive: true })
    await writeFile(targetPath, fileData)
    await updateWorkspaceBackgroundAsync(currentWorkspace.value.id, targetPath)
    add({ description: t('appearance.workspaceBackground.update.success'), color: 'success' })
  } catch (error) {
    console.error('Error selecting background:', error)
    add({ description: t('appearance.workspaceBackground.update.error'), color: 'error' })
  }
}

const removeBackgroundImage = async () => {
  if (!currentWorkspace.value) return
  try {
    if (currentWorkspace.value.background) {
      try { if (await exists(currentWorkspace.value.background)) await remove(currentWorkspace.value.background) } catch { /* ignore */ }
    }
    await updateWorkspaceBackgroundAsync(currentWorkspace.value.id, null)
    add({ description: t('appearance.workspaceBackground.remove.success'), color: 'success' })
  } catch (error) {
    console.error('Error removing background:', error)
    add({ description: t('appearance.workspaceBackground.remove.error'), color: 'error' })
  }
}

// Password
const isPasswordModalOpen = ref(false)
const isChangingPassword = ref(false)
const passwordForm = reactive({ currentPassword: '', newPassword: '', confirmPassword: '' })
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
      if (field === 'currentPassword') currentPasswordErrors.value.push(error.message)
      else if (field === 'newPassword') newPasswordErrors.value.push(error.message)
      else if (field === 'confirmPassword') confirmPasswordErrors.value.push(error.message)
    }
    return false
  }
  return true
}

const onChangePasswordAsync = async () => {
  if (!validatePasswordForm()) return
  isChangingPassword.value = true
  try {
    const result = await changePasswordAsync(passwordForm.currentPassword, passwordForm.newPassword)
    if (result.success) {
      add({ title: t('password.success.title'), description: t('password.success.description'), color: 'success' })
      isPasswordModalOpen.value = false
      resetPasswordForm()
    } else {
      if (result.error === 'Current password is incorrect') currentPasswordErrors.value = [t('password.errors.incorrect')]
      else add({ title: t('password.error.title'), description: result.error || t('password.error.description'), color: 'error' })
    }
  } catch (error) {
    console.error('Password change error:', error)
    add({ title: t('password.error.title'), description: t('password.error.description'), color: 'error' })
  } finally {
    isChangingPassword.value = false
  }
}

watch(isPasswordModalOpen, (open) => { if (!open) resetPasswordForm() })

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

onMounted(async () => {
  await checkNotificationAsync()
  await syncGradientVariantAsync()
  await syncGradientEnabledAsync()
})
</script>

<i18n lang="yaml">
de:
  title: Allgemein
  general:
    title: Grundeinstellungen
  language: Sprache
  language.description: Wähle deine bevorzugte Sprache
  vaultName:
    label: Vaultname
    description: Der Name deiner Vault
    update:
      success: Vaultname erfolgreich aktualisiert
      error: Vaultname konnte nicht aktualisiert werden
  notifications:
    label: Benachrichtigungen
    description: Erlaube Benachrichtigungen für diese App
    requestPermission: Benachrichtigung erlauben
    granted: Erlaubt
  iconSize:
    label: Icon-Größe
    description: Wähle die Größe der Desktop-Icons
    presets:
      small: Klein
      medium: Mittel
      large: Groß
      extraLarge: Sehr groß
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
  appearance:
    title: Erscheinungsbild
    design:
      label: Design
      description: Wähle zwischen hellem und dunklem Modus
    gradient:
      variant:
        label: Hintergrund-Gradient
        description: Wähle ein Farbschema für den Hintergrund
        options:
          gitlab: GitLab (Orange/Lila/Pink)
          ocean: Ozean (Blau/Türkis/Lila)
          sunset: Sonnenuntergang (Orange/Rot/Pink)
          forest: Wald (Grün/Türkis)
      enabled:
        label: Gradient aktiviert
        description: Zeige einen Farbverlauf im Hintergrund
        success: Gradient-Einstellung gespeichert
        error: Fehler beim Speichern der Gradient-Einstellung
    workspaceBackground:
      label: Workspace-Hintergrund
      description: Setze ein Hintergrundbild für deinen Workspace
      choose: Bild auswählen
      update:
        success: Hintergrund erfolgreich aktualisiert
        error: Fehler beim Aktualisieren des Hintergrunds
      remove:
        label: Hintergrund entfernen
        success: Hintergrund erfolgreich entfernt
        error: Fehler beim Entfernen des Hintergrunds
en:
  title: General
  general:
    title: Basic Settings
  language: Language
  language.description: Choose your preferred language
  vaultName:
    label: Vault Name
    description: The name of your vault
    update:
      success: Vault Name successfully updated
      error: Vault name could not be updated
  notifications:
    label: Notifications
    description: Allow notifications for this app
    requestPermission: Grant Permission
    granted: Granted
  iconSize:
    label: Icon Size
    description: Choose the size of desktop icons
    presets:
      small: Small
      medium: Medium
      large: Large
      extraLarge: Extra Large
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
  appearance:
    title: Appearance
    design:
      label: Design
      description: Choose between light and dark mode
    gradient:
      variant:
        label: Background Gradient
        description: Choose a color scheme for the background
        options:
          gitlab: GitLab (Orange/Purple/Pink)
          ocean: Ocean (Blue/Cyan/Purple)
          sunset: Sunset (Orange/Red/Pink)
          forest: Forest (Green/Cyan)
      enabled:
        label: Gradient enabled
        description: Show a gradient in the background
        success: Gradient setting saved
        error: Error saving gradient setting
    workspaceBackground:
      label: Workspace Background
      description: Set a background image for your workspace
      choose: Choose Image
      update:
        success: Background successfully updated
        error: Error updating background
      remove:
        label: Remove Background
        success: Background successfully removed
        error: Error removing background
</i18n>
