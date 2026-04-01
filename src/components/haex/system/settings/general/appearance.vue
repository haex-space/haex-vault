<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    show-back
    @back="$emit('back')"
  >
    <div class="space-y-4">
      <UFormField :label="t('design.label')" :description="t('design.description')">
        <UiSelectTheme @select="onSelectThemeAsync" />
      </UFormField>

      <UFormField :label="t('workspaceBackground.label')" :description="t('workspaceBackground.description')">
        <div class="flex gap-2">
          <UiButton :label="t('workspaceBackground.choose')" variant="outline" color="neutral" @click="selectBackgroundImage" />
          <UiButton v-if="currentWorkspace?.background" :label="t('workspaceBackground.remove.label')" color="error" @click="removeBackgroundImage" />
        </div>
      </UFormField>

      <UFormField :label="t('gradient.variant.label')" :description="t('gradient.variant.description')">
        <USelect v-model="gradientVariant" :items="gradientVariantOptions" />
      </UFormField>

      <UFormField :label="t('gradient.enabled.label')" :description="t('gradient.enabled.description')">
        <UiToggle v-model="gradientEnabled" @update:model-value="onToggleGradientAsync" />
      </UFormField>
    </div>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { open } from '@tauri-apps/plugin-dialog'
import { readFile, writeFile, mkdir, exists, remove } from '@tauri-apps/plugin-fs'
import { appLocalDataDir } from '@tauri-apps/api/path'

defineEmits<{ back: [] }>()

const { t } = useI18n()
const { add } = useToast()

const { currentThemeName } = storeToRefs(useUiStore())
const { updateThemeAsync } = useVaultSettingsStore()

const workspaceStore = useWorkspaceStore()
const { currentWorkspace } = storeToRefs(workspaceStore)
const { updateWorkspaceBackgroundAsync } = workspaceStore

const gradientStore = useGradientStore()
const { gradientVariant, gradientEnabled } = storeToRefs(gradientStore)
const { syncGradientVariantAsync, syncGradientEnabledAsync, setGradientVariantAsync, toggleGradientAsync } = gradientStore

const gradientVariantOptions = [
  { label: t('gradient.variant.options.gitlab'), value: 'gitlab' },
  { label: t('gradient.variant.options.ocean'), value: 'ocean' },
  { label: t('gradient.variant.options.sunset'), value: 'sunset' },
  { label: t('gradient.variant.options.forest'), value: 'forest' },
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
    add({ description: t('gradient.enabled.success'), color: 'success' })
  } catch (error) {
    console.error(error)
    add({ description: t('gradient.enabled.error'), color: 'error' })
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
    add({ description: t('workspaceBackground.update.success'), color: 'success' })
  } catch (error) {
    console.error('Error selecting background:', error)
    add({ description: t('workspaceBackground.update.error'), color: 'error' })
  }
}

const removeBackgroundImage = async () => {
  if (!currentWorkspace.value) return
  try {
    if (currentWorkspace.value.background) {
      try { if (await exists(currentWorkspace.value.background)) await remove(currentWorkspace.value.background) } catch { /* ignore */ }
    }
    await updateWorkspaceBackgroundAsync(currentWorkspace.value.id, null)
    add({ description: t('workspaceBackground.remove.success'), color: 'success' })
  } catch (error) {
    console.error('Error removing background:', error)
    add({ description: t('workspaceBackground.remove.error'), color: 'error' })
  }
}

onMounted(async () => {
  await syncGradientVariantAsync()
  await syncGradientEnabledAsync()
})
</script>

<i18n lang="yaml">
de:
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
