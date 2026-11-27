<script setup lang="ts">
/**
 * The width of a text field in a settings row (spec 023 FR-025): the whole width of the row on a
 * narrow row, a fixed width beside the title from `@lg` on (the row is the container). A row never hard-codes a width
 * (`w-56 max-w-full`) around its field; it wraps the field in this and picks a size. Buttons that
 * belong to the field go next to it inside this component; buttons of the row stay outside and sit
 * at the right end.
 *
 * `SettingsSelect` follows the same rule by itself, so only text fields (`UiInput`, `UiTextarea`)
 * need this wrapper. `scripts/check-vue-templates.ts` rejects a field that sits in a `SettingsRow`
 * without it.
 */
export type SettingsFieldSize = 'xs' | 'sm' | 'md' | 'lg' | 'xl'

const props = withDefaults(defineProps<{ size?: SettingsFieldSize }>(), {
  size: 'md',
})

// Whole class names, so Tailwind finds them: 7rem, 10rem, 14rem, 16rem, 18rem.
const WIDTHS: Record<SettingsFieldSize, string> = {
  xs: '@lg:w-28',
  sm: '@lg:w-40',
  md: '@lg:w-56',
  lg: '@lg:w-64',
  xl: '@lg:w-72',
}
</script>

<template>
  <div :class="['w-full min-w-0', WIDTHS[props.size]]" data-settings-field>
    <slot />
  </div>
</template>
