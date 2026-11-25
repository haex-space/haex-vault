<script setup lang="ts">
/** The copy button of a field (`usePasswordsCopy` does the copying), in a row or in the append slot
 * of an input. */
defineProps<{
  /** The field's name, for the label and the tooltip. */
  label: string
}>()
const emit = defineEmits<{ copy: [] }>()

const { t } = useI18n()
</script>

<template>
  <UiButtonCopy
    external
    :labels="{
      copy: t('passwords.copy', { field: label }),
      copied: t('fields.copied'),
    }"
    @copy="emit('copy')"
  />
</template>
