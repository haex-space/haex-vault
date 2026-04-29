<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
  >
    <template #body>
      <div class="space-y-2">
        <p
          v-if="tags.length === 0"
          class="text-sm text-muted text-center py-8"
        >
          {{ t('empty') }}
        </p>

        <div
          v-for="tag in tags"
          :key="tag.id"
          class="flex items-center gap-2 px-2 py-1.5 rounded-md hover:bg-elevated/50"
        >
          <input
            :value="tag.color ?? '#888888'"
            type="color"
            class="size-8 rounded-md border border-default cursor-pointer p-0 bg-transparent shrink-0"
            @change="onColorChange(tag, $event)"
          >
          <UiInput
            :model-value="tag.name"
            class="flex-1"
            @change="onRename(tag, $event)"
          />
          <span class="text-xs text-muted tabular-nums w-12 text-right shrink-0">
            {{ t('itemCount', { n: itemCounts.get(tag.id) ?? 0 }) }}
          </span>
          <UiButton
            :tooltip="t('delete')"
            icon="i-lucide-trash-2"
            color="error"
            variant="ghost"
            type="button"
            class="shrink-0"
            @click="askDelete(tag)"
          />
        </div>
      </div>
    </template>

    <template #footer>
      <div class="flex items-center justify-end w-full">
        <UiButton
          :label="t('close')"
          color="primary"
          type="button"
          @click="open = false"
        />
      </div>
    </template>
  </UiDrawerModal>

  <UModal
    v-model:open="showDeleteDialog"
    :title="t('deleteDialog.title')"
    :description="
      pendingDelete
        ? t('deleteDialog.description', {
          name: pendingDelete.name,
          n: itemCounts.get(pendingDelete.id) ?? 0,
        })
        : ''
    "
  >
    <template #footer>
      <div class="flex flex-col sm:flex-row gap-2 justify-end w-full">
        <UiButton
          :label="t('deleteDialog.cancel')"
          color="neutral"
          variant="outline"
          @click="showDeleteDialog = false"
        />
        <UiButton
          icon="i-lucide-trash-2"
          :label="t('deleteDialog.confirm')"
          color="error"
          @click="confirmDelete"
        />
      </div>
    </template>
  </UModal>
</template>

<script setup lang="ts">
import type { SelectHaexPasswordsTags } from '~/database/schemas'

const open = defineModel<boolean>('open', { default: false })

const { t } = useI18n()
const toast = useToast()
const tagsStore = usePasswordsTagsStore()
const passwordsStore = usePasswordsStore()
const { tags } = storeToRefs(tagsStore)

const itemCounts = ref(new Map<string, number>())
const showDeleteDialog = ref(false)
const pendingDelete = ref<SelectHaexPasswordsTags | null>(null)

const refreshAsync = async () => {
  await tagsStore.loadTagsAsync()
  itemCounts.value = await tagsStore.getItemCountsAsync()
}

watch(open, (isOpen) => {
  if (isOpen) void refreshAsync()
})

const onRename = async (tag: SelectHaexPasswordsTags, event: Event) => {
  const input = event.target as HTMLInputElement
  const next = input.value.trim()
  if (!next || next === tag.name) {
    input.value = tag.name
    return
  }
  try {
    await tagsStore.renameAsync(tag.id, next)
    await passwordsStore.loadItemsAsync()
  } catch (error) {
    console.error('[TagManager] Rename failed:', error)
    input.value = tag.name
    toast.add({
      title: t('toast.renameError'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  }
}

const onColorChange = async (tag: SelectHaexPasswordsTags, event: Event) => {
  const input = event.target as HTMLInputElement
  const next = input.value
  try {
    await tagsStore.updateColorAsync(tag.id, next)
    await passwordsStore.loadItemsAsync()
  } catch (error) {
    console.error('[TagManager] Color update failed:', error)
    toast.add({
      title: t('toast.colorError'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  }
}

const askDelete = (tag: SelectHaexPasswordsTags) => {
  pendingDelete.value = tag
  showDeleteDialog.value = true
}

const confirmDelete = async () => {
  const tag = pendingDelete.value
  if (!tag) return
  try {
    await tagsStore.deleteAsync(tag.id)
    await passwordsStore.loadItemsAsync()
    await refreshAsync()
    toast.add({ title: t('toast.deleted'), color: 'success' })
  } catch (error) {
    console.error('[TagManager] Delete failed:', error)
    toast.add({
      title: t('toast.deleteError'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  } finally {
    showDeleteDialog.value = false
    pendingDelete.value = null
  }
}
</script>

<i18n lang="yaml">
de:
  title: Tags verwalten
  empty: Noch keine Tags.
  delete: Löschen
  close: Schließen
  itemCount: "{n} Einträge"
  deleteDialog:
    title: Tag löschen?
    description: "\"{name}\" wird entfernt (aktuell von {n} Einträgen verwendet). Der Tag wird von allen betroffenen Einträgen abgelöst."
    cancel: Abbrechen
    confirm: Löschen
  toast:
    renameError: Tag konnte nicht umbenannt werden
    colorError: Farbe konnte nicht gespeichert werden
    deleted: Tag gelöscht
    deleteError: Tag konnte nicht gelöscht werden

en:
  title: Manage tags
  empty: No tags yet.
  delete: Delete
  close: Close
  itemCount: "{n} items"
  deleteDialog:
    title: Delete tag?
    description: "\"{name}\" will be removed (currently used by {n} items). The tag will be unlinked from all affected items."
    cancel: Cancel
    confirm: Delete
  toast:
    renameError: Failed to rename tag
    colorError: Failed to update color
    deleted: Tag deleted
    deleteError: Failed to delete tag
</i18n>
