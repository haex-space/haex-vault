<script setup lang="ts">
/**
 * The compact transcript under the indicator (spec 066 T048, FR-042): the last turns of the
 * conversation as they happen, the open question highlighted, and the buttons "Sprechen", mute,
 * "Gespräch zeigen" and "Gespräch beenden". Every button runs its action, so the shortcut and the
 * window do the same. The on/off switch arrives with the wake word (PR D). Escape closes it.
 */
import { lastEntries, splitSpoken } from '~/lib/assistant/bubble'

const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()
const { errString } = useErrorString()
const assistant = useAssistantStore()
const wm = useWindowManagerStore()

const entries = computed(() => lastEntries(assistant.entries))
const state = computed(() => assistant.state)
const actionError = ref<string | null>(null)

/** Kinds that are labelled; plain utterances and answers are not. */
const LABELLED = new Set([
  'notUnderstood',
  'stop',
  'closing',
  'announcement',
  'approvalQuestion',
  'approvalAnswer',
  'choiceQuestion',
  'choiceAnswer',
])

async function runAsync(id: string, input: Record<string, unknown> = {}) {
  actionError.value = null
  const outcome = await wm.runAction(id, input)
  if (!outcome.ok) actionError.value = outcome.message
}

const error = computed(
  () =>
    actionError.value ??
    (assistant.lastError === null ? null : errString(assistant.lastError)),
)

useEventListener(document, 'keydown', (event: KeyboardEvent) => {
  if (event.key === 'Escape') emit('close')
})
</script>

<template>
  <div
    class="flex w-80 max-w-[calc(100vw-2rem)] flex-col gap-3 rounded-xl border bg-popover p-3 text-popover-foreground shadow-lg"
    role="dialog"
    :aria-label="t('assistant.transcript.title')"
    data-testid="assistant-compact-transcript"
  >
    <div class="flex items-center justify-between gap-2">
      <span class="text-sm font-medium" data-testid="assistant-phase">
        {{ t(`assistant.phase.${state.phase}`) }}
      </span>
      <span
        v-if="state.muted"
        class="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground"
      >
        {{ t('assistant.muted') }}
      </span>
    </div>

    <p
      v-if="state.openQuestion"
      class="rounded-lg border border-primary/40 bg-primary/10 px-3 py-2 text-sm"
      data-testid="assistant-open-question"
    >
      {{ state.openQuestion.text }}
    </p>

    <ol
      v-if="entries.length > 0"
      class="flex max-h-64 flex-col gap-2 overflow-y-auto text-sm"
      aria-live="polite"
    >
      <li
        v-for="entry in entries"
        :key="entry.id"
        class="flex flex-col gap-0.5"
        :data-testid="`assistant-entry-${entry.kind}`"
      >
        <span class="text-xs text-muted-foreground">
          {{
            entry.speaker === 'user'
              ? t('assistant.speaker.user')
              : t('assistant.speaker.haex')
          }}
          <template v-if="LABELLED.has(entry.kind)">
            · {{ t(`assistant.kind.${entry.kind}`) }}
          </template>
        </span>
        <span>
          {{ splitSpoken(entry.text, entry.spokenUntil).spoken }}
          <span class="text-muted-foreground/60">{{
            splitSpoken(entry.text, entry.spokenUntil).unspoken
          }}</span>
        </span>
      </li>
    </ol>
    <p v-else class="text-sm text-muted-foreground">
      {{ t('assistant.transcript.empty') }}
    </p>

    <p v-if="error" class="text-xs text-destructive" role="alert">
      {{ error }}
    </p>

    <div class="flex flex-wrap gap-2">
      <UiButton
        type="button"
        size="sm"
        :loading="assistant.starting"
        data-testid="assistant-talk"
        @click="runAsync('assistant.talk')"
      >
        {{ t('assistant.buttons.talk') }}
      </UiButton>
      <UiButton
        type="button"
        size="sm"
        variant="outline"
        :disabled="!state.enabled"
        data-testid="assistant-mute"
        @click="runAsync('assistant.mute.set', { muted: !state.muted })"
      >
        {{
          state.muted
            ? t('assistant.buttons.unmute')
            : t('assistant.buttons.mute')
        }}
      </UiButton>
      <UiButton
        type="button"
        size="sm"
        variant="outline"
        :disabled="!state.threadId && !assistant.entriesThread"
        data-testid="assistant-show"
        @click="runAsync('assistant.conversation.show')"
      >
        {{ t('assistant.buttons.show') }}
      </UiButton>
      <UiButton
        type="button"
        size="sm"
        variant="outline"
        :disabled="!assistant.inConversation"
        data-testid="assistant-end"
        @click="runAsync('assistant.conversation.end')"
      >
        {{ t('assistant.buttons.end') }}
      </UiButton>
    </div>
  </div>
</template>
