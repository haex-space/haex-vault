<template>
  <UModal
    v-model:open="open"
    :title="t('title')"
    :description="t('description', { count: itemIds.length })"
  >
    <template #body>
      <div class="flex flex-col gap-4">
        <div class="flex items-center gap-2">
          <UiButton
            :label="t('modeAdd')"
            :color="mode === 'add' ? 'primary' : 'neutral'"
            :variant="mode === 'add' ? 'solid' : 'outline'"
            icon="i-lucide-plus"
            @click="mode = 'add'"
          />
          <UiButton
            :label="t('modeRemove')"
            :color="mode === 'remove' ? 'primary' : 'neutral'"
            :variant="mode === 'remove' ? 'solid' : 'outline'"
            icon="i-lucide-minus"
            @click="mode = 'remove'"
          />
        </div>

        <UInputMenu
          v-model="selectedTagId"
          v-model:search-term="tagInput"
          :items="tagOptions"
          :placeholder="t('tagPlaceholder')"
          :create-item="mode === 'add'"
          value-key="id"
          label-key="name"
          @create="onCreateTag"
        />
      </div>
    </template>
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
          :icon="mode === 'add' ? 'i-lucide-tag' : 'i-lucide-tag-x'"
          :label="mode === 'add' ? t('confirmAdd') : t('confirmRemove')"
          color="primary"
          :loading="saving"
          :disabled="!selectedTagId"
          @click="onConfirm"
        />
      </div>
    </template>
  </UModal>
</template>

<script setup lang="ts">
const props = defineProps<{
  itemIds: string[]
}>()

const open = defineModel<boolean>('open', { default: false })
const emit = defineEmits<{ confirmed: [] }>()

const { t } = useI18n()
const toast = useToast()

const tagsStore = usePasswordsTagsStore()
const selection = usePasswordsSelectionStore()
const { tags } = storeToRefs(tagsStore)

const mode = ref<'add' | 'remove'>('add')
const tagInput = ref('')
const selectedTagId = ref<string | undefined>(undefined)
const saving = ref(false)

const tagOptions = computed(() => tags.value)

watch(
  () => open.value,
  async (isOpen) => {
    if (!isOpen) return
    mode.value = 'add'
    tagInput.value = ''
    selectedTagId.value = undefined
    try {
      await tagsStore.loadTagsAsync()
    } catch (error) {
      console.error('[BulkTag] loadTags failed:', error)
    }
  },
  { immediate: true },
)

const onCreateTag = async (name: string) => {
  const trimmed = name.trim()
  if (!trimmed) return
  try {
    const tag = await tagsStore.getOrCreateTagAsync(trimmed)
    selectedTagId.value = tag.id
  } catch (error) {
    console.error('[BulkTag] create failed:', error)
    toast.add({
      title: t('toast.createError'),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  }
}

const onConfirm = async () => {
  if (!selectedTagId.value || saving.value) return
  saving.value = true
  try {
    const changed =
      mode.value === 'add'
        ? await tagsStore.bulkAddTagAsync(props.itemIds, selectedTagId.value)
        : await tagsStore.bulkRemoveTagAsync(
            props.itemIds,
            selectedTagId.value,
          )
    await usePasswordsStore().loadItemsAsync()
    selection.clear()
    toast.add({
      title:
        mode.value === 'add'
          ? t('toast.added', { count: changed })
          : t('toast.removed', { count: changed }),
      color: 'success',
    })
    open.value = false
    emit('confirmed')
  } catch (error) {
    console.error('[BulkTag] apply failed:', error)
    toast.add({
      title: t('toast.applyError'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  } finally {
    saving.value = false
  }
}
</script>

<i18n lang="yaml">
de:
  title: Tag für Auswahl
  description: "Wirkt auf {count} ausgewählte Einträge."
  modeAdd: Hinzufügen
  modeRemove: Entfernen
  tagPlaceholder: Tag eingeben oder auswählen…
  cancel: Abbrechen
  confirmAdd: Tag hinzufügen
  confirmRemove: Tag entfernen
  toast:
    added: "Tag bei {count} Einträgen hinzugefügt"
    removed: "Tag bei {count} Einträgen entfernt"
    applyError: Aktion fehlgeschlagen
    createError: Tag konnte nicht erstellt werden
en:
  title: Tag selection
  description: "Affects {count} selected entries."
  modeAdd: Add
  modeRemove: Remove
  tagPlaceholder: Enter or pick a tag…
  cancel: Cancel
  confirmAdd: Add tag
  confirmRemove: Remove tag
  toast:
    added: "Tag added to {count} entries"
    removed: "Tag removed from {count} entries"
    applyError: Action failed
    createError: Tag could not be created
</i18n>
