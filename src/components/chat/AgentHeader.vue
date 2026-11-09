<script setup lang="ts">
/**
 * The header of the agent's conversation (spec 072 FR-002a): the agent's name, the open session's
 * title, "Neue Sitzung" and, in the slot, "Verlauf" (`AgentSessionsPopover`). Only the agent has
 * sessions; contact conversations have one history each. A spoken session also gets
 * "Mitschrift" (spec 066 T127), which shows its transcript instead of its messages.
 */
import type { Thread } from '~/composables/useChat'
import { VOICE_THREAD_ICON } from '~/lib/chat/threadOrigin'

const props = defineProps<{
  threads: Thread[]
  /** The open session; none for a new, still empty one. */
  activeThreadId: string | null
  busy: boolean
}>()

const emit = defineEmits<{ newSession: [] }>()
/** Whether the transcript of a spoken session is shown instead of its messages (T127). */
const transcript = defineModel<boolean>('transcript', { default: false })

const { t } = useI18n()

const session = computed(() =>
  props.threads.find((thread) => thread.id === props.activeThreadId),
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
        <Icon
          v-if="session?.origin === 'voice'"
          :name="VOICE_THREAD_ICON"
          class="mr-1 inline h-3 w-3 align-[-2px]"
          data-testid="chat-agent-session-voice"
          :title="t('chat.threads.voice')"
          :aria-label="t('chat.threads.voice')"
        />
        {{ session?.title ?? t('chat.agent.unnamedSession') }}
      </div>
    </div>
    <UiButton
      v-if="session?.origin === 'voice'"
      variant="ghost"
      size="sm"
      class="gap-2"
      :class="{ 'bg-accent': transcript }"
      :aria-pressed="transcript"
      data-testid="chat-agent-transcript"
      :aria-label="t('assistant.transcript.title')"
      @click="transcript = !transcript"
    >
      <Icon name="lucide:scroll-text" class="h-4 w-4" />
      <span class="hidden @md:inline">{{
        t('assistant.transcript.title')
      }}</span>
    </UiButton>
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
