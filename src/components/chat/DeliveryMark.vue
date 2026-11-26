<script setup lang="ts">
/**
 * Where an own message stands (spec 072 FR-024): a clock while it waits, one check when a
 * Nostr-Relay took it, two when a device of the recipient confirmed it, two in the accent colour
 * once read, a warning when it failed. Shared by the message bubbles and the chat's list.
 */
import type { ChatDeliveryState } from '@bindings/ChatDeliveryState'

defineProps<{ state: ChatDeliveryState }>()

const { t } = useI18n()

const ICONS: Record<ChatDeliveryState, string> = {
  waiting: 'lucide:clock',
  sent: 'lucide:check',
  delivered: 'lucide:check-check',
  read: 'lucide:check-check',
  failed: 'lucide:circle-alert',
}
</script>

<template>
  <Icon
    :name="ICONS[state]"
    :class="{ 'text-primary': state === 'read' }"
    :aria-label="t(`chat.delivery.state.${state}`)"
    :title="t(`chat.delivery.state.${state}`)"
  />
</template>
