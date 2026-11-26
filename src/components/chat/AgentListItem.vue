<script setup lang="ts">
/**
 * The one "Agent" entry of the chat's list (spec 072 FR-001, revision 2026-11-01): the agent is
 * listed like a contact, always, with the title and time of its latest session; the sessions
 * themselves are reached through the agent's header ("Verlauf").
 */
import type { Thread } from '~/composables/useChat'

const props = defineProps<{
  /** The most recently active session, if there is any. */
  latest: Thread | null
  active: boolean
}>()

const emit = defineEmits<{ select: [] }>()

const { t } = useI18n()
const listTime = useListTime()

const time = computed(() =>
  props.latest ? listTime(props.latest.updatedAt) : '',
)
</script>

<template>
  <button
    type="button"
    data-testid="chat-agent"
    class="flex w-full min-w-0 items-center rounded-lg px-3 py-3 text-left transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    :class="{ 'bg-accent': active }"
    @click="emit('select')"
  >
    <ChatListRowBody
      :name="t('chat.agent.name')"
      :preview="latest?.title ?? t('chat.agent.noSession')"
      :time="time"
    >
      <template #avatar>
        <span
          class="flex size-12 shrink-0 items-center justify-center rounded-full bg-foreground text-background"
          aria-hidden="true"
        >
          <Icon name="lucide:bot" class="h-5 w-5" />
        </span>
      </template>
    </ChatListRowBody>
  </button>
</template>
