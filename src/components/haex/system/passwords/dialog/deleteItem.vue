<template>
  <UModal
    v-model:open="open"
    :title="t('title')"
    :description="t('description', { title: itemTitle || t('untitled') })"
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
          @click="$emit('confirm')"
        />
      </div>
    </template>
  </UModal>
</template>

<script setup lang="ts">
defineProps<{
  itemTitle?: string
}>()

const open = defineModel<boolean>('open', { default: false })
const { t } = useI18n()
defineEmits<{ confirm: [] }>()
</script>

<i18n lang="yaml">
de:
  title: Eintrag löschen?
  description: "\"{title}\" wird unwiderruflich gelöscht."
  untitled: (ohne Titel)
  cancel: Abbrechen
  confirm: Löschen
en:
  title: Delete entry?
  description: "\"{title}\" will be permanently deleted."
  untitled: (untitled)
  cancel: Cancel
  confirm: Delete
</i18n>
