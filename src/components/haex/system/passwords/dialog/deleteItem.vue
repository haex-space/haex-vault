<template>
  <UModal
    v-model:open="open"
    :title="final ? t('final.title') : t('title')"
    :description="final ? t('final.description', { title: itemTitle || t('untitled') }) : t('description', { title: itemTitle || t('untitled') })"
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
          @click="$emit('confirm')"
        />
      </div>
    </template>
  </UModal>
</template>

<script setup lang="ts">
defineProps<{
  itemTitle?: string
  final?: boolean
}>()

const open = defineModel<boolean>('open', { default: false })
const { t } = useI18n()
defineEmits<{ confirm: [] }>()
</script>

<i18n lang="yaml">
de:
  title: In Papierkorb verschieben?
  description: "\"{title}\" wird in den Papierkorb verschoben."
  untitled: (ohne Titel)
  cancel: Abbrechen
  confirm: In Papierkorb
  final:
    title: Endgültig löschen?
    description: "\"{title}\" wird unwiderruflich gelöscht."
    confirm: Endgültig löschen
en:
  title: Move to trash?
  description: "\"{title}\" will be moved to trash."
  untitled: (untitled)
  cancel: Cancel
  confirm: Move to trash
  final:
    title: Delete permanently?
    description: "\"{title}\" will be permanently deleted."
    confirm: Delete permanently
</i18n>
