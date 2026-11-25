<script setup lang="ts">
/**
 * "Verweis einfügen" of an editor field (spec 036, US7, research R11): an icon button for the
 * append slot of the field that opens the picker and appends the chosen placeholder to the text
 * (no cursor position). The marks below the field are `ReferenceField.vue`.
 */
const props = defineProps<{
  text: string
  /** The entry being edited, left out of the picker; `null` for a new one. */
  itemId: string | null
  /** Part of the test ids. */
  kind: string
}>()

const emit = defineEmits<{
  'update:text': [value: string]
}>()

const { t } = useI18n()
const pickerOpen = ref(false)
</script>

<template>
  <UiButtonLink
    :label="t('passwords.references.insert')"
    class="shadow-none"
    :data-testid="`passwords-reference-insert-${kind}`"
    @click.prevent="pickerOpen = true"
  />
  <PasswordsReferencePicker
    v-model:open="pickerOpen"
    :item-id="itemId"
    @pick="emit('update:text', props.text + $event)"
  />
</template>
