<template>
  <HaexSystemSettingsLayout
    :title="t('add.title')"
    show-back
    @back="$emit('back')"
  >
    <div class="space-y-3">
      <label class="text-sm font-medium">
        {{ t('add.extensionPath') }}
      </label>
      <p class="text-xs opacity-60 wrap-break-word">{{ t('add.extensionPathHint') }}</p>
      <UiInput
        v-model="extensionPath"
        :placeholder="t('add.extensionPathPlaceholder')"
        class="w-full"
      />
      <div class="flex flex-col @sm:flex-row gap-2 @sm:justify-end">
        <UiButton
          :label="t('add.browse')"
          variant="outline"
          block
          class="@sm:w-auto"
          @click="browseExtensionPathAsync"
        />
        <UiButton
          :label="t('add.loadExtension')"
          :loading="isLoading"
          :disabled="!extensionPath"
          block
          class="@sm:w-auto"
          @click="loadDevExtensionAsync"
        />
      </div>
    </div>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import type { ExtensionInfoResponse } from '~~/src-tauri/bindings/ExtensionInfoResponse'

defineEmits<{ back: [] }>()

const { t } = useI18n()
const { add } = useToast()
const { loadExtensionsAsync } = useExtensionsStore()
const windowManagerStore = useWindowManagerStore()

// State
const extensionPath = ref('')
const isLoading = ref(false)

// Browse for extension directory
const browseExtensionPathAsync = async () => {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title: t('add.browseTitle'),
    })

    if (selected && typeof selected === 'string') {
      extensionPath.value = selected
    }
  } catch (error) {
    console.error('Failed to browse directory:', error)
    add({
      description: t('add.errors.browseFailed'),
      color: 'error',
    })
  }
}

// Load a dev extension
const loadDevExtensionAsync = async () => {
  if (!extensionPath.value) return

  isLoading.value = true
  try {
    await invoke<string>('load_dev_extension', {
      extensionPath: extensionPath.value,
    })

    add({
      description: t('add.success'),
      color: 'success',
    })

    // Get all dev extensions to find the newly loaded one
    const extensions = await invoke<Array<ExtensionInfoResponse>>(
      'get_all_dev_extensions',
    )
    const newlyLoadedExtension = extensions.find((ext) =>
      extensionPath.value.includes(ext.name),
    )

    // Reload all extensions in the main extension store so they appear in the launcher
    await loadExtensionsAsync()

    // Open the newly loaded extension
    if (newlyLoadedExtension) {
      await windowManagerStore.openWindowAsync({
        sourceId: newlyLoadedExtension.id,
        type: 'extension',
        icon: newlyLoadedExtension.icon || 'i-heroicons-puzzle-piece-solid',
        title: newlyLoadedExtension.name,
      })
    }

    // Clear input
    extensionPath.value = ''
  } catch (error) {
    console.error('Failed to load dev extension:', error)
    const { getErrorMessage } = useExtensionError()
    add({
      description: `${t('add.errors.loadFailed')}: ${getErrorMessage(error)}`,
      color: 'error',
    })
  } finally {
    isLoading.value = false
  }
}
</script>

<i18n lang="yaml">
de:
  add:
    title: Dev-Extension hinzufügen
    extensionPath: Extension-Pfad
    extensionPathPlaceholder: /pfad/zu/deiner/extension
    extensionPathHint: Pfad zum Extension-Projekt (enthält haextension/ und haextension.json)
    browse: Durchsuchen
    browseTitle: Extension-Verzeichnis auswählen
    loadExtension: Extension laden
    success: Dev-Extension erfolgreich geladen
    errors:
      browseFailed: Verzeichnis konnte nicht ausgewählt werden
      loadFailed: Extension konnte nicht geladen werden
en:
  add:
    title: Add Dev Extension
    extensionPath: Extension Path
    extensionPathPlaceholder: /path/to/your/extension
    extensionPathHint: Path to your extension project (contains haextension/ and haextension.json)
    browse: Browse
    browseTitle: Select Extension Directory
    loadExtension: Load Extension
    success: Dev extension loaded successfully
    errors:
      browseFailed: Failed to select directory
      loadFailed: Failed to load extension
</i18n>
