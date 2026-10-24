<script setup lang="ts">
/**
 * A boxed list like in the COSMIC and GNOME settings (spec 023-settings-app, FR-002): rows of
 * `SettingsRow`/`SettingsOptionRow` on a rounded surface, divided by thin lines. `label` is a
 * short group name above the box, only where a view has more than one group. With `radio` the
 * list is a radio group: its rows are `SettingsOptionRow type="radio"`, `v-model` is the chosen
 * row's `value`, arrow keys move between the rows.
 */
defineProps<{
  label?: string
  radio?: boolean
  disabled?: boolean
}>()

const model = defineModel<string | number | null>({ default: null })

const LIST_CLASS =
  'flex flex-col gap-0 divide-y divide-background overflow-hidden rounded-xl bg-muted'
</script>

<template>
  <section class="flex flex-col gap-2">
    <h2 v-if="label" class="px-1 text-sm font-semibold">{{ label }}</h2>
    <ShadcnRadioGroup
      v-if="radio"
      v-model="model"
      as="ul"
      :disabled="disabled"
      :aria-label="label"
      :class="LIST_CLASS"
    >
      <slot />
    </ShadcnRadioGroup>
    <ul v-else :class="LIST_CLASS">
      <slot />
    </ul>
  </section>
</template>
