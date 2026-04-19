<template>
  <UModal
    v-model:open="open"
    :title="t('title')"
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
import type { SelectHaexPasswordsGroups } from '~/database/schemas'

const props = defineProps<{
  group: SelectHaexPasswordsGroups | null
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
    toast.add({ title: t('toast.deleted'), color: 'success' })
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
  title: Ordner löschen?
  untitled: (ohne Namen)
  descriptionEmpty: "\"{name}\" wird gelöscht."
  descriptionWithContents: "\"{name}\" inklusive {subfolders} Unterordner wird gelöscht. {items} enthaltene Einträge bleiben erhalten, werden aber nicht mehr einem Ordner zugeordnet."
  cancel: Abbrechen
  confirm: Löschen
  toast:
    deleted: Ordner gelöscht
    deleteError: Löschen fehlgeschlagen
en:
  title: Delete folder?
  untitled: (unnamed)
  descriptionEmpty: "\"{name}\" will be deleted."
  descriptionWithContents: "\"{name}\" including {subfolders} subfolders will be deleted. {items} contained entries stay but become ungrouped."
  cancel: Cancel
  confirm: Delete
  toast:
    deleted: Folder deleted
    deleteError: Delete failed
</i18n>
