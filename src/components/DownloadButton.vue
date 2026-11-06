<script setup lang="ts">
/**
 * The one way haex shows "download this" next to an entry that is not on this device yet (operator
 * 2026-11-06): a quiet download icon that takes the primary colour on hover and keyboard focus,
 * the counterpart of `DeleteButton` once the entry is here. `label` names what is downloaded, for
 * screen readers and the tooltip.
 */
const props = defineProps<{
  label: string
  loading?: boolean
  disabled?: boolean
}>()
const emit = defineEmits<{ click: [event: MouseEvent] }>()
</script>

<template>
  <UiButton
    type="button"
    variant="ghost"
    size="icon"
    class="size-8 text-muted-foreground hover:bg-primary/10 hover:text-primary focus-visible:text-primary"
    :aria-label="props.label"
    :tooltip="props.label"
    :loading="props.loading"
    :disabled="props.disabled"
    @click="emit('click', $event)"
  >
    <Icon name="lucide:download" class="size-4" aria-hidden="true" />
  </UiButton>
</template>
