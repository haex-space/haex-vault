<template>
  <div :class="variant === 'card' ? 'space-y-2' : 'divide-y divide-default'">
    <slot />
  </div>
</template>

<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    variant?: 'divider' | 'card'
  }>(),
  { variant: 'divider' },
)

provide('uiListVariant', toRef(props, 'variant'))
</script>
