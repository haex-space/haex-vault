<template>
  <UTooltip :text="formattedValue">
    <div class="bg-muted p-4 rounded-lg min-w-0">
      <div class="text-sm text-muted">
        {{ label }}
      </div>
      <div
        class="text-xl @lg:text-2xl font-bold truncate"
        :class="colorClass"
      >
        {{ formattedValue }}
      </div>
    </div>
  </UTooltip>
</template>

<script setup lang="ts">
const props = defineProps<{
  label: string
  value: string | number | bigint
  color?: 'default' | 'success' | 'warning' | 'error'
}>()

const formattedValue = computed(() => {
  if (typeof props.value === 'number') {
    return props.value.toLocaleString()
  }
  if (typeof props.value === 'bigint') {
    return props.value.toLocaleString()
  }
  return props.value
})

const colorClass = computed(() => {
  switch (props.color) {
    case 'success':
      return 'text-success'
    case 'warning':
      return 'text-warning'
    case 'error':
      return 'text-error'
    default:
      return ''
  }
})
</script>
