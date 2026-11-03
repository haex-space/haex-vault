<script setup lang="ts">
/**
 * The input field both chats share (spec 076 FR-015, research R7): the rounded box with the
 * auto-growing textarea, dictation and the send button, with a `controls` slot at the left of the
 * row below the text. The agent's field (`Composer.vue`) puts attachments, model, effort and
 * permission in the slot; a contact or group conversation (`PeerConversation.vue`) leaves it empty.
 * The `above` and `below` slots take a hint, an error or a counter in the same column as the box.
 * Presentational: sending is the parent's, this only says when the user asks for it.
 */
defineProps<{
  inputTestId: string
  sendTestId: string
  placeholder: string
  /** Names the controls row for assistive technology; omitted without controls. */
  controlsLabel?: string
  inputDisabled?: boolean
  sendDisabled?: boolean
  /** A reply is running: the cancel button takes the send button's place. */
  showCancel?: boolean
}>()

const emit = defineEmits<{
  send: []
  abort: []
  transcript: [text: string, autoSend: boolean]
}>()

const input = defineModel<string>('input', { required: true })
const voiceRecording = defineModel<boolean>('voiceRecording', {
  default: false,
})

const { t } = useI18n()

// The auto-resize composable lives here because it needs this component's own <textarea> node.
// `reset` is exposed so a parent can still trigger it after clearing the input from outside.
const { textareaRef, reset } = useAutoResizeTextarea(input)
defineExpose({ reset })

function submit() {
  if (!voiceRecording.value) emit('send')
}

// Enter confirming an input method's composition is not a send. WebKit (the Linux webview) ends
// the composition before that keydown, so only its keyCode 229 still tells it apart.
function onEnter(event: KeyboardEvent) {
  if (event.isComposing || event.keyCode === 229) return
  event.preventDefault()
  submit()
}
</script>

<template>
  <form
    class="border-t border-border bg-background/90 px-4 pb-4 pt-3 backdrop-blur md:px-8"
    @submit.prevent="submit"
  >
    <div class="mx-auto max-w-3xl">
      <slot name="above" />
      <div
        class="@container/composer rounded-2xl border border-border bg-background shadow-sm transition-shadow focus-within:border-foreground/30 focus-within:shadow-md"
      >
        <textarea
          ref="textareaRef"
          v-model="input"
          rows="1"
          class="block w-full resize-none overflow-hidden bg-transparent px-4 pb-2 pt-3 text-sm leading-6 outline-none placeholder:text-muted-foreground"
          :placeholder="placeholder"
          :aria-label="placeholder"
          :disabled="inputDisabled"
          :data-testid="inputTestId"
          @keydown.enter.exact="onEnter"
        />
        <div
          class="flex min-w-0 items-center gap-2 overflow-x-auto px-1 pb-1"
          :aria-label="controlsLabel"
        >
          <div
            class="flex min-w-0 flex-1 items-center gap-1.5 overflow-x-auto text-xs"
          >
            <slot name="controls" />
          </div>
          <ChatVoiceInputControl
            v-model:recording="voiceRecording"
            @transcript="(text, autoSend) => emit('transcript', text, autoSend)"
          />
          <template v-if="!voiceRecording">
            <UiButton
              v-if="showCancel"
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
              :data-testid="sendTestId"
            >
              <Icon name="lucide:arrow-up" class="h-3.5 w-3.5" />
            </UiButton>
          </template>
        </div>
      </div>
      <slot name="below" />
    </div>
  </form>
</template>
