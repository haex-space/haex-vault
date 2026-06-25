<template>
  <UiDialogConfirm
    :open="open"
    :title="t('renameTitle')"
    :confirm-label="t('rename')"
    :loading="loading"
    @update:open="emit('update:open', $event)"
    @confirm="emit('confirm', name)"
  >
    <template #body>
      <UiInput
        v-model="name"
        :placeholder="t('renamePlaceholder')"
        autofocus
        @keydown.enter="emit('confirm', name)"
      />
    </template>
  </UiDialogConfirm>
</template>

<script setup lang="ts">
const props = defineProps<{
  open: boolean
  loading?: boolean
  initial: string
}>()

const emit = defineEmits<{
  'update:open': [boolean]
  confirm: [newName: string]
}>()

const { t } = useI18n()

const name = ref(props.initial)

watch(
  () => props.open,
  (v) => {
    if (v) name.value = props.initial
  },
)
</script>

<i18n lang="yaml">
de:
  rename: Umbenennen
  renameTitle: Datei umbenennen
  renamePlaceholder: Neuer Name
en:
  rename: Rename
  renameTitle: Rename file
  renamePlaceholder: New name
</i18n>
