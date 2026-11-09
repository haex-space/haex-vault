<script setup lang="ts">
/**
 * The one "Agent" entry of the chat's list (spec 072 FR-001, revision 2026-11-01): the agent is
 * listed like a contact, always, with the title and time of its latest session; the sessions
 * themselves are reached through the agent's header ("Verlauf").
 */
import type { Thread } from '~/composables/useChat'
import { listTime } from '~/lib/chat/listTime'

const props = defineProps<{
  /** The most recently active session, if there is any. */
  latest: Thread | null
  active: boolean
}>()

const emit = defineEmits<{ select: [] }>()

const { t, d } = useI18n()

const time = computed(() =>
  props.latest ? listTime(props.latest.updatedAt, d) : '',
)
</script>

<template>
  <button
    type="button"
    data-testid="chat-agent"
    class="flex w-full min-w-0 items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    :class="{ 'bg-accent font-medium': active }"
    @click="emit('select')"
  >
    <span
      class="flex size-9 shrink-0 items-center justify-center rounded-full bg-foreground text-background"
      aria-hidden="true"
    >
      <Icon name="lucide:bot" class="h-4 w-4" />
    </span>
    <span class="min-w-0 flex-1">
      <span class="block truncate">{{ t('chat.agent.name') }}</span>
      <span class="block truncate text-xs font-normal text-muted-foreground">
        {{ latest?.title ?? t('chat.agent.noSession') }}
      </span>
    </span>
    <span
      v-if="time"
      class="shrink-0 self-start pt-0.5 text-xs font-normal tabular-nums text-muted-foreground"
    >
      {{ time }}
    </span>
  </button>
</template>
