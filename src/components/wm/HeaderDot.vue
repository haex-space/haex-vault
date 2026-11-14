<script setup lang="ts">
/**
 * The dot of an active control (spec 093-app-headers, FR-008): a menu whose filter, sort or view is
 * not at its default shows it, so a filtered list is recognisable without opening the menu. `corner`
 * pins it to the top right of its (relative) parent; without it, it sits inline. With a `label` it
 * names the state for assistive technology; the dot alone is `aria-hidden`.
 */
defineProps<{ corner?: boolean; label?: string }>()
</script>

<template>
  <span
    :class="[
      'size-1.5 shrink-0 rounded-full bg-primary',
      corner
        ? 'pointer-events-none absolute top-1.5 right-1.5'
        : 'inline-block',
    ]"
    :role="label ? 'img' : undefined"
    :aria-label="label"
    :aria-hidden="label ? undefined : true"
    data-testid="wm-header-dot"
  />
</template>
