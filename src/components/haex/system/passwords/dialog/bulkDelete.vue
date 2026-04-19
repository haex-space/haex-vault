<template>
  <UModal
    v-model:open="open"
    :title="t('title')"
    :description="description"
  >
    <template #footer>
      <div class="flex flex-col sm:flex-row gap-2 justify-end w-full">
        <UiButton
          icon="i-lucide-x"
          :label="t('cancel')"
          color="neutral"
          variant="outline"
          @click="open = false"
        />
        <UiButton
          icon="i-lucide-trash-2"
          :label="t('confirm')"
          color="error"
          variant="solid"
          :loading="deleting"
          @click="onConfirm"
        />
      </div>
    </template>
  </UModal>
</template>

<script setup lang="ts">
import type { SelectionEntry } from '~/stores/passwords/selection'

const props = defineProps<{
  entries: SelectionEntry[]
}>()

const open = defineModel<boolean>('open', { default: false })
const emit = defineEmits<{ confirmed: [] }>()

const { t } = useI18n()
const toast = useToast()

const groupsStore = usePasswordsGroupsStore()
const passwordsStore = usePasswordsStore()
const selection = usePasswordsSelectionStore()

const deleting = ref(false)

const counts = computed(() => {
  let items = 0
  let folders = 0
  for (const entry of props.entries) {
    if (entry.type === 'item') items++
    else folders++
  }
  return { items, folders }
})

const description = computed(() => {
  const { items, folders } = counts.value
  if (folders === 0) return t('descriptionItemsOnly', { count: items })
  if (items === 0) return t('descriptionFoldersOnly', { count: folders })
  return t('descriptionMixed', { items, folders })
})

const onConfirm = async () => {
  if (deleting.value) return
  deleting.value = true
  try {
    await groupsStore.bulkDeleteAsync(props.entries)
    await passwordsStore.loadItemsAsync()
    selection.clear()
    toast.add({ title: t('toast.deleted'), color: 'success' })
    open.value = false
    emit('confirmed')
  } catch (error) {
    console.error('[BulkDelete] failed:', error)
    toast.add({
      title: t('toast.deleteError'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  } finally {
    deleting.value = false
  }
}
</script>

<i18n lang="yaml">
de:
  title: Auswahl löschen?
  descriptionItemsOnly: "{count} Einträge werden gelöscht."
  descriptionFoldersOnly: "{count} Ordner werden gelöscht. Enthaltene Einträge bleiben erhalten, werden aber nicht mehr einem Ordner zugeordnet."
  descriptionMixed: "{items} Einträge und {folders} Ordner werden gelöscht. Enthaltene Einträge in den Ordnern bleiben erhalten (ungrouped)."
  cancel: Abbrechen
  confirm: Löschen
  toast:
    deleted: Auswahl gelöscht
    deleteError: Löschen fehlgeschlagen
en:
  title: Delete selection?
  descriptionItemsOnly: "{count} entries will be deleted."
  descriptionFoldersOnly: "{count} folders will be deleted. Contained entries stay but become ungrouped."
  descriptionMixed: "{items} entries and {folders} folders will be deleted. Contained entries in folders stay (ungrouped)."
  cancel: Cancel
  confirm: Delete
  toast:
    deleted: Selection deleted
    deleteError: Delete failed
</i18n>
