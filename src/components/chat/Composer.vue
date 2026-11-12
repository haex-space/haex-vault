<script setup lang="ts">
/**
 * The agent's input row: the shared field (`ComposerShell.vue`: textarea, voice input,
 * send/cancel) with attachments, model/effort picker and permission control in its
 * controls slot (spec 076; the shell was extracted from this file). Extracted from
 * `src/pages/chat/[instance].vue` (spec 015-workspace-shell, T012, plan
 * research R10). Presentational: wires the existing `Chat*` composer
 * controls together and forwards their events; `useComposer`/
 * `useComposerAttachments` stay page-level. Model/effort display reads
 * `useModelsStore()` directly (a global singleton, same instance the page
 * already uses) rather than threading its fields through as props.
 */
import type { ComposerAttachment } from '~/components/chat/ComposerAttachments.vue'
import type { PastedFiles } from '~/lib/chat/pastedAttachments'
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
  pasteFiles: [paste: PastedFiles]
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

// Spec 083 FR-006b: the active agent profile decides which models the chat offers. The store
// follows the vault, so changing the profile (here or in the settings) changes the list at once.
const profileStore = useAgentProfilesStore()
const { unavailableModelIds } = storeToRefs(profileStore)
onMounted(() => {
  void profileStore.reloadQuietlyAsync()
})
const offeredGroups = computed(() =>
  modelGroups.value
    .map((group) => ({
      ...group,
      models: group.models.filter(
        (model) => !unavailableModelIds.value.has(model.id),
      ),
    }))
    .filter((group) => group.models.length > 0),
)

const modelDisabled = computed(
  () => offeredGroups.value.length === 0 || modelLoadPending.value,
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

// The shell owns the <textarea> and its auto-resize. `reset` is exposed so the page (via
// useComposer/useThreadSidebar, which take it as a plain `() => Promise<void>` dependency) can still
// trigger it after clearing the input from outside this component (new chat, thread delete).
const shell = useTemplateRef<{ reset: () => Promise<void> }>('shell')
defineExpose({ reset: async () => shell.value?.reset() })
</script>

<template>
  <ChatComposerShell
    ref="shell"
    v-model:input="input"
    v-model:voice-recording="voiceRecording"
    input-test-id="chat-input"
    send-test-id="chat-send"
    :placeholder="t('chat.composer.placeholder')"
    :controls-label="t('chat.composer.settingsLabel')"
    :input-disabled="composerInputDisabled"
    :send-disabled="sendDisabled"
    :show-cancel="!!streamingMessageId || turnSetupPending"
    :accept-pasted-files="!busy"
    @send="emit('send')"
    @abort="emit('abort')"
    @paste-files="emit('pasteFiles', $event)"
    @transcript="(text, autoSend) => emit('transcript', text, autoSend)"
  >
    <template #attachments>
      <ChatComposerAttachmentList
        v-if="attachments.length"
        :attachments="attachments"
        @remove="emit('removeAttachment', $event)"
      />
    </template>

    <template #controls>
      <ChatComposerAttachments
        :disabled="busy"
        @add="emit('addAttachments', $event)"
      />

      <ChatAgentActivityIndicator
        :count="activeAgentCount"
        :last-batch-size="lastAgentBatchSize"
      />

      <ChatComposerSettingsPopover
        :model-id="displayModelId"
        :model-name="displayModelName"
        :model-groups="offeredGroups"
        :effort-choices="effortChoices"
        :effort-level="effortLevel"
        :effort-label="effortLabel"
        :effort-state="effortState"
        :disabled="busy"
        :model-disabled="modelDisabled"
        @update:model-id="
          selectModel(
            props.threadId
              ? { modelId: $event, threadId: props.threadId }
              : { modelId: $event },
          )
        "
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
    </template>
  </ChatComposerShell>
</template>
