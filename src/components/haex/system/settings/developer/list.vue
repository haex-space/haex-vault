<template>
  <HaexSystemSettingsLayout
    :title="t('list.title')"
    show-back
    @back="$emit('back')"
  >
    <UiListContainer v-if="devExtensions.length > 0">
      <UiListItem
        v-for="ext in devExtensions"
        :key="ext.id"
      >
        <div class="space-y-1 min-w-0">
          <div class="flex items-center gap-2 flex-wrap">
            <h4 class="font-medium">{{ ext.name }}</h4>
            <UBadge color="info">DEV</UBadge>
          </div>
          <p class="text-sm opacity-70">v{{ ext.version }}</p>
          <p class="text-xs opacity-50 truncate">
            {{ ext.publicKey.slice(0, 16) }}...
          </p>
        </div>

        <template #actions>
          <UiButton
            :label="t('list.remove')"
            variant="ghost"
            color="error"
            @click="removeDevExtensionAsync(ext)"
          />
        </template>
      </UiListItem>
    </UiListContainer>

    <HaexSystemSettingsLayoutEmpty
      v-else
      :message="t('list.empty')"
      icon="i-lucide-puzzle"
    />
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import type { ExtensionInfoResponse } from '~~/src-tauri/bindings/ExtensionInfoResponse'

defineEmits<{ back: [] }>()

const { t } = useI18n()
const { add } = useToast()
const { loadExtensionsAsync } = useExtensionsStore()

const devExtensions = ref<Array<ExtensionInfoResponse>>([])

// Load dev extensions on mount
onMounted(async () => {
  await loadDevExtensionListAsync()
})

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
  list:
    title: Geladene Dev-Extensions
    empty: Keine Dev-Extensions geladen
    remove: Entfernen
    removeSuccess: Dev-Extension erfolgreich entfernt
    errors:
      removeFailed: Extension konnte nicht entfernt werden
en:
  list:
    title: Loaded Dev Extensions
    empty: No dev extensions loaded
    remove: Remove
    removeSuccess: Dev extension removed successfully
    errors:
      removeFailed: Failed to remove extension
</i18n>
