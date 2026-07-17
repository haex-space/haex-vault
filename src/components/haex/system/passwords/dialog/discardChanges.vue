<template>
  <UModal
    v-model:open="open"
    :title="t('title')"
    :description="t('description')"
  >
    <template #footer>
      <div class="flex flex-col sm:flex-row gap-2 justify-end w-full">
        <UiButton
          icon="i-lucide-arrow-left"
          :label="t('cancel')"
          color="neutral"
          variant="outline"
          :disabled="saving"
          @click="() => { open = false }"
        />
        <UiButton
          icon="i-lucide-trash-2"
          :label="t('confirm')"
          color="warning"
          variant="solid"
          :disabled="saving"
          @click="$emit('confirm')"
        />
        <UiButton
          icon="i-lucide-save"
          :label="t('save')"
          color="primary"
          variant="solid"
          :loading="saving"
          :disabled="saving"
          @click="$emit('save')"
        />
      </div>
    </template>
  </UModal>
</template>

<script setup lang="ts">
const open = defineModel<boolean>('open', { default: false })
defineProps<{ saving?: boolean }>()
const { t } = useI18n()
defineEmits<{ confirm: []; save: [] }>()
</script>

<i18n lang="yaml">
de:
  title: Änderungen verwerfen?
  description: Ungespeicherte Änderungen gehen verloren.
  cancel: Weiter bearbeiten
  confirm: Verwerfen
  save: Speichern
en:
  title: Discard changes?
  description: Unsaved changes will be lost.
  cancel: Keep editing
  confirm: Discard
  save: Save
</i18n>
