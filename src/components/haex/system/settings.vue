<template>
  <HaexSystem :is-dragging="isDragging">
    <template #sidebar>
      <nav class="space-y-1">
        <button
          v-for="category in categories"
          :key="category.value"
          :class="[
            'w-full flex items-center justify-center @md:justify-start gap-3 px-2 @md:px-3 py-2 text-sm font-medium rounded-md transition-colors',
            category.active
              ? 'bg-primary text-white'
              : 'text-highlighted hover:bg-muted'
          ]"
          :title="category.label"
          @click="category.click"
        >
          <Icon :name="category.icon" class="w-5 h-5 shrink-0" />
          <span class="hidden @md:inline">{{ category.label }}</span>
        </button>
      </nav>
    </template>

    <div class="flex-1 overflow-y-auto">
      <!-- General Settings -->
      <div v-if="activeCategory === 'general'">
        <div class="p-6 border-b border-base-content/10">
          <h2 class="text-2xl font-bold">
            {{ t('categories.general') }}
          </h2>
        </div>

        <div class="p-6 space-y-6">
          <UFormField
            :label="t('language')"
            :description="t('language.description')"
          >
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
        </div>
      </div>

      <!-- Appearance Settings -->
      <div v-if="activeCategory === 'appearance'">
        <div class="p-6 border-b border-base-content/10">
          <h2 class="text-2xl font-bold">
            {{ t('categories.appearance') }}
          </h2>
        </div>

        <div class="p-6 space-y-6">
          <UFormField
            :label="t('design')"
            :description="t('design.description')"
          >
            <UiDropdownTheme @select="onSelectThemeAsync" />
          </UFormField>

          <UFormField
            :label="t('workspaceBackground.label')"
            :description="t('workspaceBackground.description')"
          >
            <div class="flex gap-2">
              <UiButton
                :label="t('workspaceBackground.choose')"
                @click="selectBackgroundImage"
              />
              <UiButton
                v-if="currentWorkspace?.background"
                :label="t('workspaceBackground.remove.label')"
                color="error"
                @click="removeBackgroundImage"
              />
            </div>
          </UFormField>

          <UFormField
            :label="t('gradient.variant.label')"
            :description="t('gradient.variant.description')"
          >
            <USelect
              v-model="gradientVariant"
              :items="gradientVariantOptions"
            />
          </UFormField>

          <UFormField
            :label="t('gradient.enabled.label')"
            :description="t('gradient.enabled.description')"
          >
            <UiToggle
              v-model="gradientEnabled"
              @update:model-value="onToggleGradientAsync"
            />
          </UFormField>
        </div>
      </div>

      <!-- Workspace Settings -->
      <div v-if="activeCategory === 'workspace'">
        <div class="p-6 border-b border-base-content/10">
          <h2 class="text-2xl font-bold">
            {{ t('categories.workspace') }}
          </h2>
        </div>

        <div class="p-6 space-y-6">
          <UFormField
            :label="t('desktopGrid.iconSize.label')"
            :description="t('desktopGrid.iconSize.description')"
          >
            <USelect
              v-model="iconSizePreset"
              :items="iconSizePresetOptions"
            />
          </UFormField>
        </div>
      </div>

      <!-- Notifications Settings -->
      <div v-if="activeCategory === 'notifications'">
        <div class="p-6 border-b border-base-content/10">
          <h2 class="text-2xl font-bold">
            {{ t('categories.notifications') }}
          </h2>
        </div>

        <div class="p-6 space-y-6">
          <UFormField
            :label="t('notifications.label')"
            :description="t('notifications.description')"
          >
            <UiButton
              :label="t('notifications.requestPermission')"
              @click="requestNotificationPermissionAsync"
            />
          </UFormField>
        </div>
      </div>
    </div>
  </HaexSystem>
</template>

<script setup lang="ts">
import type { Locale } from 'vue-i18n'
import { open } from '@tauri-apps/plugin-dialog'
import {
  readFile,
  writeFile,
  mkdir,
  exists,
  remove,
} from '@tauri-apps/plugin-fs'
import { appLocalDataDir } from '@tauri-apps/api/path'
import { DesktopIconSizePreset } from '~/stores/vault/settings'

defineProps<{
  isDragging?: boolean
}>()

const { t, setLocale } = useI18n()

// Active category
const activeCategory = ref('general')

// Categories for sidebar navigation - computed to make active state reactive
const categories = computed(() => [
  {
    value: 'general',
    label: t('categories.general'),
    icon: 'i-heroicons-cog-6-tooth',
    active: activeCategory.value === 'general',
    click: () => {
      activeCategory.value = 'general'
    },
  },
  {
    value: 'appearance',
    label: t('categories.appearance'),
    icon: 'i-heroicons-paint-brush',
    active: activeCategory.value === 'appearance',
    click: () => {
      activeCategory.value = 'appearance'
    },
  },
  {
    value: 'workspace',
    label: t('categories.workspace'),
    icon: 'i-heroicons-squares-2x2',
    active: activeCategory.value === 'workspace',
    click: () => {
      activeCategory.value = 'workspace'
    },
  },
  {
    value: 'notifications',
    label: t('categories.notifications'),
    icon: 'i-heroicons-bell',
    active: activeCategory.value === 'notifications',
    click: () => {
      activeCategory.value = 'notifications'
    },
  },
])

const { currentVaultName } = storeToRefs(useVaultStore())
const { updateVaultNameAsync, updateLocaleAsync, updateThemeAsync } =
  useVaultSettingsStore()

const onSelectLocaleAsync = async (locale: Locale) => {
  await updateLocaleAsync(locale)
  await setLocale(locale)
}

const { currentThemeName } = storeToRefs(useUiStore())

const onSelectThemeAsync = async (theme: string) => {
  currentThemeName.value = theme
  console.log('onSelectThemeAsync', currentThemeName.value)
  await updateThemeAsync(theme)
}

const { add } = useToast()

const onSetVaultNameAsync = async () => {
  try {
    await updateVaultNameAsync(currentVaultName.value)
    add({ description: t('vaultName.update.success'), color: 'success' })
  } catch (error) {
    console.error(error)
    add({ description: t('vaultName.update.error'), color: 'error' })
  }
}

const { requestNotificationPermissionAsync } = useNotificationStore()

const { deviceName } = storeToRefs(useDeviceStore())
const { updateDeviceNameAsync, readDeviceNameAsync } = useDeviceStore()

const workspaceStore = useWorkspaceStore()
const { currentWorkspace } = storeToRefs(workspaceStore)
const { updateWorkspaceBackgroundAsync } = workspaceStore

const desktopStore = useDesktopStore()
const { iconSizePreset } = storeToRefs(desktopStore)
const { syncDesktopIconSizeAsync, updateDesktopIconSizeAsync } = desktopStore

const gradientStore = useGradientStore()
const { gradientVariant, gradientEnabled } = storeToRefs(gradientStore)
const {
  syncGradientVariantAsync,
  syncGradientEnabledAsync,
  setGradientVariantAsync,
  toggleGradientAsync,
} = gradientStore

// Icon size preset options
const iconSizePresetOptions = [
  {
    label: t('desktopGrid.iconSize.presets.small'),
    value: DesktopIconSizePreset.small,
  },
  {
    label: t('desktopGrid.iconSize.presets.medium'),
    value: DesktopIconSizePreset.medium,
  },
  {
    label: t('desktopGrid.iconSize.presets.large'),
    value: DesktopIconSizePreset.large,
  },
  {
    label: t('desktopGrid.iconSize.presets.extraLarge'),
    value: DesktopIconSizePreset.extraLarge,
  },
]

// Gradient variant options
const gradientVariantOptions = [
  {
    label: t('gradient.variant.options.gitlab'),
    value: 'gitlab',
  },
  {
    label: t('gradient.variant.options.ocean'),
    value: 'ocean',
  },
  {
    label: t('gradient.variant.options.sunset'),
    value: 'sunset',
  },
  {
    label: t('gradient.variant.options.forest'),
    value: 'forest',
  },
]

// Watch for icon size preset changes and update DB
watch(iconSizePreset, async (newPreset) => {
  if (newPreset) {
    await updateDesktopIconSizeAsync(newPreset)
  }
})

// Watch for gradient variant changes and update DB
watch(gradientVariant, async (newVariant) => {
  if (newVariant) {
    await setGradientVariantAsync(newVariant)
  }
})

// Handler for gradient toggle
const onToggleGradientAsync = async (enabled: boolean) => {
  try {
    await toggleGradientAsync(enabled)
    add({ description: t('gradient.enabled.success'), color: 'success' })
  } catch (error) {
    console.error(error)
    add({ description: t('gradient.enabled.error'), color: 'error' })
  }
}

onMounted(async () => {
  await readDeviceNameAsync()
  await syncDesktopIconSizeAsync()
  await syncGradientVariantAsync()
  await syncGradientEnabledAsync()
})

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

const selectBackgroundImage = async () => {
  if (!currentWorkspace.value) return

  try {
    const selected = await open({
      multiple: false,
      filters: [
        {
          name: 'Images',
          extensions: ['png', 'jpg', 'jpeg', 'webp'],
        },
      ],
    })

    if (!selected || typeof selected !== 'string') {
      return
    }

    // Read the selected file (works with Android photo picker URIs)
    let fileData: Uint8Array
    try {
      fileData = await readFile(selected)
    } catch (readError) {
      add({
        description: `Fehler beim Lesen: ${readError instanceof Error ? readError.message : String(readError)}`,
        color: 'error',
      })
      return
    }

    // Detect file type from file signature
    let ext = 'jpg' // default
    if (fileData.length > 4) {
      // PNG signature: 89 50 4E 47
      if (
        fileData[0] === 0x89 &&
        fileData[1] === 0x50 &&
        fileData[2] === 0x4e &&
        fileData[3] === 0x47
      ) {
        ext = 'png'
      }
      // JPEG signature: FF D8 FF
      else if (
        fileData[0] === 0xff &&
        fileData[1] === 0xd8 &&
        fileData[2] === 0xff
      ) {
        ext = 'jpg'
      }
      // WebP signature: RIFF xxxx WEBP
      else if (
        fileData[0] === 0x52 &&
        fileData[1] === 0x49 &&
        fileData[2] === 0x46 &&
        fileData[3] === 0x46
      ) {
        ext = 'webp'
      }
    }

    // Get app local data directory
    const appDataPath = await appLocalDataDir()

    // Construct target path manually to avoid path joining issues
    const fileName = `workspace-${currentWorkspace.value.id}-background.${ext}`
    const targetPath = `${appDataPath}/files/${fileName}`

    // Create parent directory if it doesn't exist
    const parentDir = `${appDataPath}/files`
    try {
      if (!(await exists(parentDir))) {
        await mkdir(parentDir, { recursive: true })
      }
    } catch (mkdirError) {
      add({
        description: `Fehler beim Erstellen des Ordners: ${mkdirError instanceof Error ? mkdirError.message : String(mkdirError)}`,
        color: 'error',
      })
      return
    }

    // Write file to app data directory
    try {
      await writeFile(targetPath, fileData)
    } catch (writeError) {
      add({
        description: `Fehler beim Schreiben: ${writeError instanceof Error ? writeError.message : String(writeError)}`,
        color: 'error',
      })
      return
    }

    // Store the absolute file path in database
    try {
      await updateWorkspaceBackgroundAsync(
        currentWorkspace.value.id,
        targetPath,
      )
      add({
        description: t('workspaceBackground.update.success'),
        color: 'success',
      })
    } catch (dbError) {
      add({
        description: `Fehler beim DB-Update: ${dbError instanceof Error ? dbError.message : String(dbError)}`,
        color: 'error',
      })
    }
  } catch (error) {
    console.error('Error selecting background:', error)
    add({
      description: `${t('workspaceBackground.update.error')}: ${error instanceof Error ? error.message : String(error)}`,
      color: 'error',
    })
  }
}

const removeBackgroundImage = async () => {
  if (!currentWorkspace.value) return

  try {
    // Delete the background file if it exists
    if (currentWorkspace.value.background) {
      try {
        // The background field contains the absolute file path
        if (await exists(currentWorkspace.value.background)) {
          await remove(currentWorkspace.value.background)
        }
      } catch (err) {
        console.warn('Could not delete background file:', err)
        // Continue anyway to clear the database entry
      }
    }

    await updateWorkspaceBackgroundAsync(currentWorkspace.value.id, null)
    add({
      description: t('workspaceBackground.remove.success'),
      color: 'success',
    })
  } catch (error) {
    console.error('Error removing background:', error)
    add({ description: t('workspaceBackground.remove.error'), color: 'error' })
  }
}
</script>

<i18n lang="yaml">
de:
  categories:
    general: Allgemein
    appearance: Erscheinungsbild
    workspace: Arbeitsbereich
    notifications: Benachrichtigungen
  language: Sprache
  language.description: Wähle deine bevorzugte Sprache
  design: Design
  design.description: Wähle zwischen hellem und dunklem Modus
  save: Änderung speichern
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
  notifications:
    label: Benachrichtigungen
    description: Erlaube Benachrichtigungen für diese App
    requestPermission: Benachrichtigung erlauben
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
  desktopGrid:
    title: Desktop-Raster
    iconSize:
      label: Icon-Größe
      description: Wähle die Größe der Desktop-Icons
      presets:
        small: Klein
        medium: Mittel
        large: Groß
        extraLarge: Sehr groß
      unit: px
en:
  categories:
    general: General
    appearance: Appearance
    workspace: Workspace
    notifications: Notifications
  language: Language
  language.description: Choose your preferred language
  design: Design
  design.description: Choose between light and dark mode
  save: save changes
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
  notifications:
    label: Notifications
    description: Allow notifications for this app
    requestPermission: Grant Permission
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
  desktopGrid:
    title: Desktop Grid
    iconSize:
      label: Icon Size
      description: Choose the size of desktop icons
      presets:
        small: Small
        medium: Medium
        large: Large
        extraLarge: Extra Large
      unit: px
</i18n>
