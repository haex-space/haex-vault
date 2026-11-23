<script setup lang="ts">
/**
 * What a conversation's protection state means (spec 095 US4): the state in plain words (scenario
 * 1), when the keys were last renewed, what Nostr-Relays can see and do (FR-014, scenario 2) and
 * that the kept history is outside this protection (FR-024).
 */
import type { ChatPeerProtection } from '@bindings/ChatPeerProtection'
import { PROTECTION_LIMITS, PROTECTION_VIEWS } from '~/lib/chat/protection'

const props = defineProps<{ protection: ChatPeerProtection }>()

const { t, locale } = useI18n()

const view = computed(() => PROTECTION_VIEWS[props.protection.state])
const renewedAt = computed(() =>
  props.protection.renewedAt === null
    ? null
    : new Date(props.protection.renewedAt).toLocaleString(locale.value, {
        dateStyle: 'medium',
        timeStyle: 'short',
      }),
)
</script>

<template>
  <div
    class="flex flex-col gap-2 border-b border-border bg-muted/40 px-4 py-3 text-sm md:px-8"
    data-testid="chat-peer-protection-explanation"
  >
    <p class="flex items-start gap-2">
      <Icon
        :name="view.icon"
        class="mt-0.5 h-4 w-4 shrink-0"
        :class="view.warning ? 'text-warning' : 'text-muted-foreground'"
      />
      <span>
        <span class="font-medium">{{ t(view.label) }}.</span>
        {{ t(view.explanation) }}
      </span>
    </p>
    <p v-if="renewedAt" class="text-xs text-muted-foreground">
      {{ t('chat.peer.protection.renewedAt', { date: renewedAt }) }}
    </p>
    <p
      v-for="limit in PROTECTION_LIMITS"
      :key="limit"
      class="text-xs text-muted-foreground"
    >
      {{ t(limit) }}
    </p>
  </div>
</template>
