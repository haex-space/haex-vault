<template>
  <UModal
    v-model:open="open"
    :title="final ? t('final.title') : t('title')"
    :description="descriptionText"
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
import type { SelectHaexPasswordsGroups } from '~/database/schemas'

const props = defineProps<{
  group: SelectHaexPasswordsGroups | null
  final?: boolean
}>()

const open = defineModel<boolean>('open', { default: false })
const { t } = useI18n()
const toast = useToast()

const groupsStore = usePasswordsGroupsStore()
const { itemGroupMap } = storeToRefs(groupsStore)
const deleting = ref(false)

const descendantCount = computed(() => {
  if (!props.group) return 0
  const set = groupsStore.descendantIdSet(props.group.id)
  return set.size - 1
})

const itemsInScope = computed(() => {
  if (!props.group) return 0
  const set = groupsStore.descendantIdSet(props.group.id)
  let count = 0
  for (const groupId of itemGroupMap.value.values()) {
    if (groupId && set.has(groupId)) count++
  }
  return count
})

const descriptionText = computed(() => {
  if (!props.group) return ''
  const name = props.group.name ?? t('untitled')
  if (props.final) {
    if (descendantCount.value === 0 && itemsInScope.value === 0) {
      return t('final.descriptionEmpty', { name })
    }
    return t('final.descriptionWithContents', {
      name,
      subfolders: descendantCount.value,
      items: itemsInScope.value,
    })
  }
  if (descendantCount.value === 0 && itemsInScope.value === 0) {
    return t('descriptionEmpty', { name })
  }
  return t('descriptionWithContents', {
    name,
    subfolders: descendantCount.value,
    items: itemsInScope.value,
  })
})

const onConfirm = async () => {
  if (!props.group || deleting.value) return
  deleting.value = true
  try {
    await groupsStore.deleteGroupAsync(props.group.id)
    toast.add({ title: props.final ? t('toast.deleted') : t('toast.movedToTrash'), color: 'success' })
    open.value = false
  } catch (error) {
    console.error('[DeleteGroup] failed:', error)
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
  title: In Papierkorb verschieben?
  untitled: (ohne Namen)
  descriptionEmpty: "\"{name}\" wird in den Papierkorb verschoben."
  descriptionWithContents: "\"{name}\" wird inklusive {subfolders} Unterordner und {items} Einträgen in den Papierkorb verschoben."
  cancel: Abbrechen
  confirm: In Papierkorb
  toast:
    movedToTrash: In Papierkorb verschoben
    deleted: Ordner gelöscht
    deleteError: Löschen fehlgeschlagen
  final:
    title: Ordner endgültig löschen?
    descriptionEmpty: "\"{name}\" wird unwiderruflich gelöscht."
    descriptionWithContents: "\"{name}\" wird unwiderruflich gelöscht. Alle {subfolders} Unterordner und {items} Einträge werden ebenfalls gelöscht."
    confirm: Endgültig löschen
en:
  title: Move to trash?
  untitled: (unnamed)
  descriptionEmpty: "\"{name}\" will be moved to trash."
  descriptionWithContents: "\"{name}\" including {subfolders} subfolders and {items} entries will be moved to trash."
  cancel: Cancel
  confirm: Move to trash
  toast:
    movedToTrash: Moved to trash
    deleted: Folder deleted
    deleteError: Delete failed
  final:
    title: Delete folder permanently?
    descriptionEmpty: "\"{name}\" will be permanently deleted."
    descriptionWithContents: "\"{name}\" will be permanently deleted along with {subfolders} subfolders and {items} entries."
    confirm: Delete permanently
</i18n>
