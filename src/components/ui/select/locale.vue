<template>
  <USelect
    :model-value="locale"
    :items="items"
    :icon="flags[locale]"
    @update:model-value="emit('select', $event)"
  />
</template>

<script setup lang="ts">
import type { Locale } from 'vue-i18n'

const { locales, locale } = useI18n()

const flags: Record<string, string> = {
  de: 'circle-flags:de',
  en: 'circle-flags:uk',
}

const emit = defineEmits<{ select: [Locale] }>()

const items = computed(() =>
  locales.value.map((l) => ({
    label: l.code,
    value: l.code,
    icon: flags[l.code],
  })),
)
</script>
