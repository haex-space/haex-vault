<template>
  <div class="space-y-4 mt-4">
    <UiInput
      v-model="form.label"
      :label="t('fields.label')"
      :placeholder="t('manual.labelPlaceholder')"
    />
    <UiTextarea
      v-model="form.publicKey"
      :label="t('fields.publicKey')"
      :placeholder="t('manual.publicKeyPlaceholder')"
      :rows="3"
    />
    <UiTextarea
      v-model="form.notes"
      :label="t('fields.notes')"
      :placeholder="t('manual.notesPlaceholder')"
      :rows="2"
    />
  </div>
</template>

<script setup lang="ts">
defineProps<{
  form: { label: string; publicKey: string; notes: string }
}>()

const { t } = useI18n()
</script>
