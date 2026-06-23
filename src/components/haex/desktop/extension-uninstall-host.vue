<template>
  <HaexExtensionDialogRemove
    v-model:open="showRemoveDialog"
    :extension="extensionToRemove"
    @confirm="handleRemoveExtension"
  />
</template>

<script setup lang="ts">
import type { IHaexSpaceExtension } from '~/types/haexspace'

const extensionsStore = useExtensionsStore()

const showRemoveDialog = ref(false)
const extensionToRemove = ref<IHaexSpaceExtension | undefined>(undefined)

const requestUninstall = (extensionId: string) => {
  const extension = extensionsStore.availableExtensions.find(
    (ext) => ext.id === extensionId,
  )

  if (extension) {
    extensionToRemove.value = extension
    showRemoveDialog.value = true
  }
}

const handleRemoveExtension = async (deleteMode: 'device' | 'complete') => {
  if (!extensionToRemove.value) return

  try {
    // Uninstall extension (handles dev/regular, removes desktop items, reloads list)
    await extensionsStore.uninstallExtensionAsync(extensionToRemove.value.id, deleteMode)
  } catch (error) {
    console.error('Failed to remove extension:', error)
  }
}

defineExpose({ requestUninstall })
</script>
