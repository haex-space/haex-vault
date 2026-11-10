<script setup lang="ts">
/**
 * The reasoning-effort select of the composer's settings (spec 012): the popover of a wide screen
 * and the drawer of a small one both show it, so there is one place that knows its states.
 * `selectable` offers the options plus Auto; `managed` and `unknown` show a disabled select that
 * says why there is nothing to choose; `hidden` renders nothing.
 */
import { computed } from 'vue'
import type { EffortState } from '~/composables/useReasoningPreference'

/** Sentinel `UiSelect` value for "no override" (`effortLevel: null`) —
 * Reka UI's Select works over strings, so `null` never appears on the wire
 * between this component and its select. */
const AUTO_VALUE = 'auto'

const props = defineProps<{
  id: string
  /** Options the displayed model offers, already labelled for display. Only
   * used while `effortState` is `selectable`. */
  effortChoices: { id: string; label: string }[]
  /** The effective option id; `null` is Auto. */
  effortLevel: string | null
  effortState: EffortState
  disabled?: boolean
}>()

const emit = defineEmits<{
  'update:effortLevel': [effortLevel: string | null]
}>()

const { t } = useI18n()

const effortOptions = computed(() => [
  { value: AUTO_VALUE, label: t('chat.effort.auto') },
  ...props.effortChoices.map((choice) => ({
    value: choice.id,
    label: choice.label,
  })),
])

function updateEffort(value: unknown) {
  if (typeof value !== 'string') return
  emit('update:effortLevel', value === AUTO_VALUE ? null : value)
}
</script>

<template>
  <div v-if="effortState !== 'hidden'" class="mt-1">
    <label
      :for="id"
      class="mb-1 block text-xs font-medium text-muted-foreground"
    >
      {{ t('chat.composer.settingsPopover.effortLabel') }}
    </label>
    <UiSelect
      v-if="effortState === 'selectable'"
      :id="id"
      :model-value="effortLevel ?? AUTO_VALUE"
      :options="effortOptions"
      :aria-label="t('chat.composer.settingsPopover.effortLabel')"
      :disabled="disabled"
      @update:model-value="updateEffort"
    />
    <!-- Nothing to choose, but worth saying why: the model reasons on
         its own, or its capabilities are not known yet (spec 012). -->
    <UiSelect
      v-else
      :id="id"
      :options="[]"
      :placeholder="
        effortState === 'managed'
          ? t('chat.effort.managed')
          : t('chat.effort.unknown')
      "
      :aria-label="t('chat.composer.settingsPopover.effortLabel')"
      disabled
    />
  </div>
</template>
