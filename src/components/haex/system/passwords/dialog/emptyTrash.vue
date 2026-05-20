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
          :loading="emptying"
          :disabled="itemCount === 0 && groupCount === 0"
          @click="onConfirm"
        />
      </div>
    </template>
  </UModal>
</template>

<script setup lang="ts">
const props = defineProps<{
  itemCount: number
  groupCount: number
}>()

const open = defineModel<boolean>('open', { default: false })

const { t } = useI18n()
const toast = useToast()

const groupsStore = usePasswordsGroupsStore()
const passwordsStore = usePasswordsStore()

const emptying = ref(false)

const description = computed(() => {
  const { itemCount, groupCount } = props
  if (itemCount === 0 && groupCount === 0) return t('descriptionEmpty')
  if (groupCount === 0) return t('descriptionItemsOnly', { count: itemCount })
  if (itemCount === 0) return t('descriptionFoldersOnly', { count: groupCount })
  return t('descriptionMixed', { items: itemCount, folders: groupCount })
})

const onConfirm = async () => {
  if (emptying.value) return
  emptying.value = true
  try {
    await groupsStore.emptyTrashAsync()
    await passwordsStore.loadItemsAsync()
    toast.add({ title: t('toast.emptied'), color: 'success' })
    open.value = false
  } catch (error) {
    console.error('[EmptyTrash] failed:', error)
    toast.add({
      title: t('toast.error'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  } finally {
    emptying.value = false
  }
}
</script>

<i18n lang="yaml">
de:
  title: Papierkorb endgültig leeren?
  descriptionEmpty: Der Papierkorb ist bereits leer.
  descriptionItemsOnly: "{count} Einträge werden unwiderruflich gelöscht."
  descriptionFoldersOnly: "{count} Ordner werden inklusive aller Inhalte unwiderruflich gelöscht."
  descriptionMixed: "{items} Einträge und {folders} Ordner werden unwiderruflich gelöscht."
  cancel: Abbrechen
  confirm: Papierkorb leeren
  toast:
    emptied: Papierkorb geleert
    error: Papierkorb konnte nicht geleert werden
en:
  title: Empty trash permanently?
  descriptionEmpty: The trash is already empty.
  descriptionItemsOnly: "{count} entries will be permanently deleted."
  descriptionFoldersOnly: "{count} folders including all contents will be permanently deleted."
  descriptionMixed: "{items} entries and {folders} folders will be permanently deleted."
  cancel: Cancel
  confirm: Empty trash
  toast:
    emptied: Trash emptied
    error: Failed to empty trash
</i18n>
