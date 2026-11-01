<script setup lang="ts">
/**
 * Composer input row (textarea, attachments, model/effort picker,
 * permission control, voice input, send/cancel) — extracted from
 * `src/pages/chat/[instance].vue` (spec 015-workspace-shell, T012, plan
 * research R10). Presentational: wires the existing `Chat*` composer
 * controls together and forwards their events; `useComposer`/
 * `useComposerAttachments` stay page-level. Model/effort display reads
 * `useModelsStore()` directly (a global singleton, same instance the page
 * already uses) rather than threading its fields through as props.
 */
import type { ComposerAttachment } from '~/components/chat/ComposerAttachments.vue'
import type { ChoiceAnswer, PendingPrompt } from '~/lib/chat/prompts'

const props = defineProps<{
  composerInputDisabled: boolean
  sendDisabled: boolean
  streamingMessageId: string | null
  turnSetupPending: boolean
  busy: boolean
  attachments: ComposerAttachment[]
  activeAgentCount: number
  lastAgentBatchSize: number | null
  permissionMode: 'manual' | 'auto' | 'plan'
  pendingPrompts: PendingPrompt[]
  /** The open thread, for its delegate's calls to holzi (spec 021 US6). */
  threadId?: string | null
  permissionDisabled: boolean
}>()

const emit = defineEmits<{
  send: []
  abort: []
  addAttachments: [paths: string[]]
  removeAttachment: [id: string]
  updatePermissionMode: [mode: 'manual' | 'auto' | 'plan']
  respondApproval: [requestId: string, decision: 'allow' | 'deny']
  answerChoice: [requestId: string, answer: ChoiceAnswer]
  transcript: [text: string, autoSend: boolean]
}>()

const input = defineModel<string>('input', { required: true })
const voiceRecording = defineModel<boolean>('voiceRecording', {
  default: false,
})

const { t } = useI18n()
const modelStore = useModelsStore()

/** The agent's question, when it is the oldest prompt (spec 046); approvals have their own dialog. */
const choice = computed(() => {
  const first = props.pendingPrompts[0]
  return first?.kind === 'choice' ? first : undefined
})
const {
  displayModelId,
  displayModelName,
  modelGroups,
  modelLoadPending,
  effortLevel,
  effortOptions,
  effortState,
} = storeToRefs(modelStore)
// Spec 020 FR-024: model and reasoning choices run their catalog actions.
const selectModel = useAction('chat.model.select')
const setReasoning = useAction('chat.reasoning.set')

const modelDisabled = computed(
  () => modelGroups.value.length === 0 || modelLoadPending.value,
)
/** An option's display text: the known-id translation, else the provider's own label. */
function effortOptionLabel(option: { id: string; label: string }): string {
  const key = `chat.effort.${option.id}`
  const translated = t(key)
  return translated === key ? option.label : translated
}
const effortChoices = computed(() =>
  effortOptions.value.map((option) => ({
    id: option.id,
    label: effortOptionLabel(option),
  })),
)
// Trigger text for the active choice; empty unless the control is selectable.
const effortLabel = computed(() => {
  if (effortState.value !== 'selectable') return ''
  const id = effortLevel.value
  if (id === null) return t('chat.effort.auto')
  return effortChoices.value.find((choice) => choice.id === id)?.label ?? id
})

// The auto-resize composable lives here, not on the page, because it needs
// direct access to this component's own <textarea> DOM node. `reset` is
// exposed so the page (via useComposer/useThreadSidebar, which take it as a
// plain `() => Promise<void>` dependency) can still trigger it after
// clearing the input from outside this component (new chat, thread delete).
const { textareaRef, reset } = useAutoResizeTextarea(input)
defineExpose({ reset })
</script>

<template>
  <form
    class="border-t border-border bg-background/90 px-4 pb-4 pt-3 backdrop-blur md:px-8"
    @submit.prevent="!voiceRecording && emit('send')"
  >
    <div class="mx-auto max-w-3xl">
      <div
        class="@container/composer rounded-2xl border border-border bg-background shadow-sm transition-shadow focus-within:border-foreground/30 focus-within:shadow-md"
      >
        <textarea
          ref="textareaRef"
          v-model="input"
          rows="1"
          class="block w-full resize-none overflow-hidden bg-transparent px-4 pb-2 pt-3 text-sm leading-6 outline-none placeholder:text-muted-foreground"
          :placeholder="t('chat.composer.placeholder')"
          :disabled="composerInputDisabled"
          data-testid="chat-input"
          @keydown.enter.exact.prevent="!voiceRecording && emit('send')"
        />
        <div
          class="flex min-w-0 items-center gap-2 overflow-x-auto px-1 pb-1"
          :aria-label="t('chat.composer.settingsLabel')"
        >
          <div
            class="flex min-w-0 flex-1 items-center gap-1.5 overflow-x-auto text-xs"
          >
            <ChatComposerAttachments
              :attachments="attachments"
              :disabled="busy"
              @add="emit('addAttachments', $event)"
              @remove="emit('removeAttachment', $event)"
            />

            <ChatAgentActivityIndicator
              :count="activeAgentCount"
              :last-batch-size="lastAgentBatchSize"
            />

            <ChatComposerSettingsPopover
              :model-id="displayModelId"
              :model-name="displayModelName"
              :model-groups="modelGroups"
              :effort-choices="effortChoices"
              :effort-level="effortLevel"
              :effort-label="effortLabel"
              :effort-state="effortState"
              :disabled="busy"
              :model-disabled="modelDisabled"
              @update:model-id="selectModel({ modelId: $event })"
              @update:effort-level="
                setReasoning($event === null ? {} : { level: $event })
              "
            />

            <ChatPermissionPrompt
              :mode="permissionMode"
              :pending-prompts="pendingPrompts"
              :thread-id="threadId"
              :disabled="permissionDisabled"
              @update:mode="emit('updatePermissionMode', $event)"
              @allow="emit('respondApproval', $event, 'allow')"
              @deny="emit('respondApproval', $event, 'deny')"
              @cancel="emit('abort')"
            />
            <ChatChoicePrompt
              v-if="choice"
              :prompt="choice"
              @answer="(id, answer) => emit('answerChoice', id, answer)"
            />
          </div>
          <ChatVoiceInputControl
            v-model:recording="voiceRecording"
            @transcript="(text, autoSend) => emit('transcript', text, autoSend)"
          />
          <template v-if="!voiceRecording">
            <UiButton
              v-if="streamingMessageId || turnSetupPending"
              class="shrink-0"
              size="icon-sm"
              variant="destructive"
              type="button"
              :aria-label="t('chat.cancel')"
              :title="t('chat.cancel')"
              @click="emit('abort')"
            >
              <Icon name="lucide:square" class="h-3.5 w-3.5 fill-current" />
            </UiButton>
            <UiButton
              v-else
              class="shrink-0"
              size="icon-sm"
              type="submit"
              :disabled="!input.trim() || sendDisabled"
              :aria-label="t('chat.send')"
              :title="t('chat.send')"
              data-testid="chat-send"
            >
              <Icon name="lucide:arrow-up" class="h-3.5 w-3.5" />
            </UiButton>
          </template>
        </div>
      </div>
    </div>
  </form>
</template>
