<template>
  <USelect
    :model-value="currentTheme?.value"
    :items="items"
    :icon="currentTheme?.icon"
    class="w-40"
    @update:model-value="emit('select', $event)"
  />
</template>

<script setup lang="ts">
const { availableThemes, currentTheme } = storeToRefs(useUiStore())

const emit = defineEmits<{ select: [string] }>()

const items = computed(() =>
  availableThemes.value.map((theme) => ({
    label: theme.label,
    value: theme.value,
    icon: theme.icon,
  })),
)
</script>
