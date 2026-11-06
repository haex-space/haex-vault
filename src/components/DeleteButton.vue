<script setup lang="ts">
/**
 * The one way haex shows "delete this" next to an entry (operator 2026-11-06): a quiet bin icon
 * that turns red on hover and keyboard focus. `label` names what goes, for screen readers and the
 * tooltip. With `placeholder` it keeps its place but stays invisible and out of reach, so rows with
 * and without a delete button line up; it is not disabled then, since a row that holds a disabled
 * control looks greyed out.
 */
const props = defineProps<{
  label: string
  loading?: boolean
  disabled?: boolean
  placeholder?: boolean
}>()
const emit = defineEmits<{ click: [event: MouseEvent] }>()
</script>

<template>
  <UiButton
    type="button"
    variant="ghost"
    size="icon"
    class="size-8 text-muted-foreground hover:bg-destructive/10 hover:text-destructive focus-visible:text-destructive"
    :class="{ invisible: props.placeholder }"
    :aria-label="props.placeholder ? undefined : props.label"
    :aria-hidden="props.placeholder ? 'true' : undefined"
    :tabindex="props.placeholder ? -1 : undefined"
    :tooltip="props.placeholder ? undefined : props.label"
    :loading="props.loading"
    :disabled="props.disabled && !props.placeholder"
    @click="emit('click', $event)"
  >
    <Icon name="lucide:trash-2" class="size-4" aria-hidden="true" />
  </UiButton>
</template>
