<script setup lang="ts">
/**
 * The header of the agent's conversation (spec 072 FR-002a): the agent's name, the open session's
 * title, "Neue Sitzung" and, in the slot, "Verlauf" (`AgentSessionsPopover`). Only the agent has
 * sessions; contact conversations have one history each.
 */
import type { Thread } from '~/composables/useChat'

const props = defineProps<{
  threads: Thread[]
  /** The open session; none for a new, still empty one. */
  activeThreadId: string | null
  busy: boolean
}>()

const emit = defineEmits<{ newSession: [] }>()

const { t } = useI18n()

const sessionTitle = computed(
  () =>
    props.threads.find((thread) => thread.id === props.activeThreadId)?.title,
)
</script>

<template>
  <header
    class="flex shrink-0 items-center gap-3 border-b border-border px-4 py-2 md:px-8"
    data-testid="chat-agent-header"
  >
    <span
      class="flex size-9 shrink-0 items-center justify-center rounded-full bg-foreground text-background"
      aria-hidden="true"
    >
      <Icon name="lucide:bot" class="h-4 w-4" />
    </span>
    <div class="min-w-0 flex-1">
      <div class="truncate text-sm font-semibold">
        {{ t('chat.agent.name') }}
      </div>
      <div
        class="truncate text-xs text-muted-foreground"
        data-testid="chat-agent-session"
      >
        {{ sessionTitle ?? t('chat.agent.unnamedSession') }}
      </div>
    </div>
    <UiButton
      variant="ghost"
      size="sm"
      class="gap-2"
      :disabled="busy"
      data-testid="chat-new"
      :aria-label="t('chat.agent.newSession')"
      @click="emit('newSession')"
    >
      <Icon name="lucide:plus" class="h-4 w-4" />
      <span class="hidden @md:inline">{{ t('chat.agent.newSession') }}</span>
    </UiButton>
    <slot />
  </header>
</template>
