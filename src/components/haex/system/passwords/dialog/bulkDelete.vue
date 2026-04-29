<template>
  <UModal
    v-model:open="open"
    :title="final ? t('final.title') : t('title')"
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
          :icon="final ? 'i-lucide-trash-2' : 'i-lucide-trash'"
          :label="final ? t('final.confirm') : t('confirm')"
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
  final?: boolean
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
  if (props.final) {
    if (folders === 0) return t('final.descriptionItemsOnly', { count: items })
    if (items === 0) return t('final.descriptionFoldersOnly', { count: folders })
    return t('final.descriptionMixed', { items, folders })
  }
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
    toast.add({ title: props.final ? t('toast.deleted') : t('toast.movedToTrash'), color: 'success' })
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
  title: Auswahl in Papierkorb?
  descriptionItemsOnly: "{count} Einträge werden in den Papierkorb verschoben."
  descriptionFoldersOnly: "{count} Ordner werden inklusive ihrer Inhalte in den Papierkorb verschoben."
  descriptionMixed: "{items} Einträge und {folders} Ordner werden in den Papierkorb verschoben."
  cancel: Abbrechen
  confirm: In Papierkorb
  toast:
    movedToTrash: In Papierkorb verschoben
    deleted: Auswahl gelöscht
    deleteError: Löschen fehlgeschlagen
  final:
    title: Auswahl endgültig löschen?
    descriptionItemsOnly: "{count} Einträge werden unwiderruflich gelöscht."
    descriptionFoldersOnly: "{count} Ordner werden inklusive aller Inhalte unwiderruflich gelöscht."
    descriptionMixed: "{items} Einträge und {folders} Ordner werden unwiderruflich gelöscht."
    confirm: Endgültig löschen
en:
  title: Move selection to trash?
  descriptionItemsOnly: "{count} entries will be moved to trash."
  descriptionFoldersOnly: "{count} folders including their contents will be moved to trash."
  descriptionMixed: "{items} entries and {folders} folders will be moved to trash."
  cancel: Cancel
  confirm: Move to trash
  toast:
    movedToTrash: Moved to trash
    deleted: Selection deleted
    deleteError: Delete failed
  final:
    title: Delete selection permanently?
    descriptionItemsOnly: "{count} entries will be permanently deleted."
    descriptionFoldersOnly: "{count} folders including all contents will be permanently deleted."
    descriptionMixed: "{items} entries and {folders} folders will be permanently deleted."
    confirm: Delete permanently
</i18n>
