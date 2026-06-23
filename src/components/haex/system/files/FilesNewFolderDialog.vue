<template>
  <UiDialogConfirm
    :open="open"
    :title="t('newFolder')"
    :confirm-label="t('create')"
    :loading="loading"
    @update:open="emit('update:open', $event)"
    @confirm="emit('confirm', name)"
  >
    <template #body>
      <UiInput
        v-model="name"
        :placeholder="t('folderNamePlaceholder')"
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
}>()

const emit = defineEmits<{
  'update:open': [boolean]
  confirm: [name: string]
}>()

const { t } = useI18n()

const name = ref('')

watch(
  () => props.open,
  (v) => {
    if (v) name.value = ''
  },
)
</script>
