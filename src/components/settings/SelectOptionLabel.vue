<script setup lang="ts">
/**
 * What an option of a `SettingsSelect` shows: its label and, after it, its badges. The select's
 * item puts all of this inside its item text, so the badges stand in the list and, for the chosen
 * option, in the closed select too. An option may start with an icon (an image URL or an icon name).
 */
import type { SettingsSelectBadge } from '~/components/settings/Select.vue'

defineProps<{
  label: string
  badges?: readonly SettingsSelectBadge[]
  icon?: string
  iconUrl?: string
}>()

const TONES: Record<SettingsSelectBadge['tone'], string> = {
  success: 'bg-success/10 text-success',
  warning: 'bg-warning/10 text-warning',
}
</script>

<template>
  <span class="flex min-w-0 items-center gap-2">
    <img
      v-if="iconUrl"
      :src="iconUrl"
      alt=""
      class="size-5 shrink-0 object-contain"
      draggable="false"
    />
    <Icon v-else-if="icon" :name="icon" class="size-5 shrink-0" />
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
