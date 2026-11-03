<script setup lang="ts">
/**
 * What an option of a `SettingsSelect` shows: its label and, after it, its badges. The select's
 * item puts all of this inside its item text, so the badges stand in the list and, for the chosen
 * option, in the closed select too.
 */
import type { SettingsSelectBadge } from '~/components/settings/Select.vue'

defineProps<{
  label: string
  badges?: readonly SettingsSelectBadge[]
}>()

const TONES: Record<SettingsSelectBadge['tone'], string> = {
  success: 'bg-success/10 text-success',
  warning: 'bg-warning/10 text-warning',
}
</script>

<template>
  <span class="flex min-w-0 items-center gap-2">
    <span class="truncate">{{ label }}</span>
    <span
      v-for="badge in badges ?? []"
      :key="badge.text"
      class="shrink-0 rounded-full px-2 py-0.5 text-xs"
      :class="TONES[badge.tone]"
    >
      {{ badge.text }}
    </span>
  </span>
</template>
