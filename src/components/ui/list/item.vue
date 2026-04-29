<template>
  <div
    class="py-3 first:pt-0 last:pb-0"
    :class="[$attrs.class, highlight && 'bg-primary/10 px-3 rounded-lg -mx-3 border border-primary/20']"
    v-bind="listeners"
  >
    <div class="flex items-center justify-between gap-3">
      <div class="flex-1 min-w-0">
        <slot />
      </div>
      <div v-if="$slots.actions" class="shrink-0">
        <slot name="actions" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
defineOptions({ inheritAttrs: false })

withDefaults(
  defineProps<{
    highlight?: boolean
  }>(),
  {
    highlight: false,
  },
)

// Forward event listeners (onClick etc.) to the root element
const attrs = useAttrs()
const listeners = computed(() => {
  const result: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(attrs)) {
    if (key.startsWith('on') && typeof value === 'function') {
      result[key] = value
    }
  }
  return result
})
</script>
