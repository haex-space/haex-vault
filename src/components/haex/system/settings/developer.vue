<template>
  <div class="space-y-6">
    <div class="px-6 pt-6">
      <h2 class="text-xl font-semibold">
        {{ t('title') }}
      </h2>
      <p class="text-sm text-base-content/60 mt-1">
        {{ t('description') }}
      </p>
    </div>

    <div class="@container p-6 space-y-6">
      <!-- Add Dev Extension Form -->
      <UCard class="p-4 space-y-4">
        <h3 class="text-lg font-semibold">{{ t('add.title') }}</h3>

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
      </UCard>

      <!-- List of Dev Extensions -->
      <div
        v-if="devExtensions.length > 0"
        class="space-y-2"
      >
        <h3 class="text-lg font-semibold">{{ t('list.title') }}</h3>

        <UCard
          v-for="ext in devExtensions"
          :key="ext.id"
          class="p-4 flex items-center justify-between"
        >
          <div class="space-y-1">
            <div class="flex items-center gap-2">
              <h4 class="font-medium">{{ ext.name }}</h4>
              <UBadge color="info">DEV</UBadge>
            </div>
            <p class="text-sm opacity-70">v{{ ext.version }}</p>
            <p class="text-xs opacity-50">
              {{ ext.publicKey.slice(0, 16) }}...
            </p>
          </div>

          <div class="flex gap-2">
            <UiButton
              :label="t('list.reload')"
              variant="outline"
              size="sm"
              @click="reloadDevExtensionAsync(ext)"
            />
            <UiButton
              :label="t('list.remove')"
              variant="ghost"
              size="sm"
              color="error"
              @click="removeDevExtensionAsync(ext)"
            />
          </div>
        </UCard>
      </div>

      <div
        v-else
        class="text-center py-8 opacity-50"
      >
        {{ t('list.empty') }}
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import type { ExtensionInfoResponse } from '~~/src-tauri/bindings/ExtensionInfoResponse'

const { t } = useI18n()
const { add } = useToast()
const { loadExtensionsAsync } = useExtensionsStore()

// State
const extensionPath = ref('')
const isLoading = ref(false)
const devExtensions = ref<Array<ExtensionInfoResponse>>([])

// Load dev extensions on mount
onMounted(async () => {
  await loadDevExtensionListAsync()
})

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

const windowManagerStore = useWindowManagerStore()

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

    // Reload list
    await loadDevExtensionListAsync()

    // Get the newly loaded extension info from devExtensions
    const newlyLoadedExtension = devExtensions.value.find((ext) =>
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

// Load all dev extensions (for the list on this page)
const loadDevExtensionListAsync = async () => {
  try {
    const extensions = await invoke<Array<ExtensionInfoResponse>>(
      'get_all_dev_extensions',
    )
    devExtensions.value = extensions
  } catch (error) {
    console.error('Failed to load dev extensions:', error)
  }
}

// Reload a dev extension (removes and re-adds)
const reloadDevExtensionAsync = async (extension: ExtensionInfoResponse) => {
  try {
    console.log('reloadDevExtensionAsync', extension)
    // Get the extension path from somewhere (we need to store this)
    // For now, just show a message
    add({
      description: t('list.reloadInfo'),
      color: 'info',
    })
  } catch (error) {
    console.error('Failed to reload dev extension:', error)
    const { getErrorMessage } = useExtensionError()
    add({
      description: `${t('list.errors.reloadFailed')}: ${getErrorMessage(error)}`,
      color: 'error',
    })
  }
}

// Remove a dev extension
const removeDevExtensionAsync = async (extension: ExtensionInfoResponse) => {
  try {
    await invoke('remove_dev_extension', {
      publicKey: extension.publicKey,
      name: extension.name,
    })

    add({
      description: t('list.removeSuccess'),
      color: 'success',
    })

    // Reload list
    await loadDevExtensionListAsync()

    // Reload all extensions store
    await loadExtensionsAsync()
  } catch (error) {
    console.error('Failed to remove dev extension:', error)
    const { getErrorMessage } = useExtensionError()
    add({
      description: `${t('list.errors.removeFailed')}: ${getErrorMessage(error)}`,
      color: 'error',
    })
  }
}
</script>

<i18n lang="yaml">
de:
  title: Entwickler
  description: Lade Extensions im Entwicklungsmodus für schnelleres Testen mit Hot-Reload.
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
  list:
    title: Geladene Dev-Extensions
    empty: Keine Dev-Extensions geladen
    reload: Neu laden
    remove: Entfernen
    reloadInfo: Extension wird beim nächsten Laden automatisch aktualisiert
    removeSuccess: Dev-Extension erfolgreich entfernt
    errors:
      reloadFailed: Extension konnte nicht neu geladen werden
      removeFailed: Extension konnte nicht entfernt werden

en:
  title: Developer
  description: Load extensions in development mode for faster testing with hot-reload.
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
  list:
    title: Loaded Dev Extensions
    empty: No dev extensions loaded
    reload: Reload
    remove: Remove
    reloadInfo: Extension will be automatically updated on next load
    removeSuccess: Dev extension removed successfully
    errors:
      reloadFailed: Failed to reload extension
      removeFailed: Failed to remove extension
</i18n>
