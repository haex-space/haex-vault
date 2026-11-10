<script setup lang="ts">
/**
 * A contact or group conversation (spec 072 US1, US2, FR-006): header with picture and name, the
 * messages and the agent's input field (`ComposerShell.vue`) with text and dictation, without any
 * model, effort, permission or agent control.
 * Sending and sending again run their catalog actions (`chat.peer.send`, `chat.peer.resend`);
 * the message appears at once as _waiting_ and moves on as the delivery goes. A request (US5) shows
 * its answers in place of the composer: nothing is sent to its sender before it is accepted.
 */
import { toast } from 'vue-sonner'
import type { PeerConversationEntry } from '~/composables/useConversations'
import {
  MAX_TEXT_CHARS,
  usePeerConversation,
} from '~/composables/usePeerConversation'

const props = defineProps<{
  conversation: PeerConversationEntry | null
  /** Runs `chat.peer.send`; resolves with an error text when it failed. */
  send: (
    threadId: string,
    text: string,
    idempotencyKey: string,
  ) => Promise<string | null>
  /** Runs `chat.peer.resend`; resolves with an error text when it failed. */
  resend: (messageId: string) => Promise<string | null>
}>()

const emit = defineEmits<{
  mute: [threadId: string, muted: boolean]
  delete: [threadId: string]
  answer: [threadId: string, answer: 'accept' | 'decline' | 'block']
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const draft = ref('')
const confirmDelete = ref(false)
const sending = ref(false)
/** One key per message: a retry of the same draft sends it once (contracts/ipc.md). */
let idempotencyKey = crypto.randomUUID()

const threadId = computed(() => props.conversation?.id ?? null)
const peer = usePeerConversation(threadId, errString)
// A failed load or read mark of the conversation shows as a toast.
watch(peer.error, (message) => {
  if (message) toast.error(message)
})

const length = computed(() => [...draft.value].length)
const tooLong = computed(() => length.value > MAX_TEXT_CHARS)
const canSend = computed(
  () =>
    !sending.value &&
    !tooLong.value &&
    draft.value.trim().length > 0 &&
    props.conversation !== null,
)

async function submit() {
  const id = threadId.value
  if (!id || !canSend.value) return
  const text = draft.value
  sending.value = true
  const error = await props.send(id, text, idempotencyKey)
  sending.value = false
  if (error !== null) toast.error(error)
  if (threadId.value !== id) return
  // Text typed or dictated while the send ran is a new message: it stays in the field and gets
  // its own key, so neither a success clears it nor a retry sends it under the old key.
  const draftChanged = draft.value !== text
  if (error === null && !draftChanged) draft.value = ''
  if (error === null || draftChanged) idempotencyKey = crypto.randomUUID()
}

/** Dictation (072 FR-006, 076 FR-016): the text goes into the field and, with auto-send on, is sent
 * like typed text, so the same limits apply. The field is keyed by conversation: switching ends a
 * running dictation, so its text cannot land in, or be sent to, the next conversation. */
function onTranscript(text: string, autoSend: boolean) {
  draft.value = text
  if (autoSend) void submit()
}

async function resendMessage(messageId: string) {
  const error = await props.resend(messageId)
  if (error !== null) toast.error(error)
}

function deleteConversation() {
  const id = props.conversation?.id
  confirmDelete.value = false
  if (id) emit('delete', id)
}

watch(
  () => props.conversation?.id,
  () => {
    draft.value = ''
    idempotencyKey = crypto.randomUUID()
    confirmDelete.value = false
  },
)
</script>

<template>
  <section
    class="flex min-h-0 min-w-0 flex-1 flex-col"
    data-testid="chat-peer-conversation"
  >
    <template v-if="conversation">
      <header
        class="flex items-center gap-3 border-b border-border px-4 py-3 md:px-8"
      >
        <ContactsAvatar
          :name="conversation.displayName"
          :photo="conversation.photo"
        />
        <div class="min-w-0 flex-1">
          <h2 class="truncate text-base font-semibold">
            {{ conversation.displayName }}
          </h2>
          <p
            v-if="conversation.muted"
            class="flex items-center gap-1 text-xs text-muted-foreground"
          >
            <Icon name="lucide:bell-off" class="h-3 w-3" />
            {{ t('chat.list.muted') }}
          </p>
        </div>
        <!-- Notes never notify, so they have nothing to mute (FR-002b); a request is answered. -->
        <UiButton
          v-if="
            conversation.kind !== 'note' && conversation.state !== 'request'
          "
          type="button"
          variant="ghost"
          size="icon"
          :aria-label="
            conversation.muted ? t('chat.peer.unmute') : t('chat.peer.mute')
          "
          :tooltip="
            conversation.muted ? t('chat.peer.unmute') : t('chat.peer.mute')
          "
          data-testid="chat-peer-mute"
          @click="emit('mute', conversation.id, !conversation.muted)"
        >
          <Icon
            :name="conversation.muted ? 'lucide:bell' : 'lucide:bell-off'"
            class="h-4 w-4"
          />
        </UiButton>
        <UiButton
          type="button"
          variant="ghost"
          size="icon"
          :aria-label="t('chat.peer.delete')"
          :tooltip="t('chat.peer.delete')"
          data-testid="chat-peer-delete"
          @click="confirmDelete = true"
        >
          <Icon name="lucide:trash-2" class="h-4 w-4" />
        </UiButton>
      </header>

      <div
        class="flex min-h-0 flex-1 flex-col-reverse overflow-y-auto px-4 py-6 md:px-8"
      >
        <ChatPeerMessageList
          v-if="peer.messages.value.length > 0"
          :messages="peer.messages.value"
          :peer-name="conversation.displayName"
          :peer-photo="conversation.photo"
          @resend="resendMessage"
        />
        <p v-else class="m-auto text-sm text-muted-foreground">
          {{
            conversation.kind === 'note'
              ? t('chat.notes.empty')
              : t('chat.peer.noMessages')
          }}
        </p>
      </div>

      <div
        v-if="conversation.state === 'request'"
        class="border-t border-border px-4 py-3 md:px-8"
      >
        <ChatRequestBanner
          :name="conversation.displayName"
          @accept="emit('answer', conversation.id, 'accept')"
          @decline="emit('answer', conversation.id, 'decline')"
          @block="emit('answer', conversation.id, 'block')"
        />
      </div>
      <ChatComposerShell
        v-else
        :key="conversation.id"
        v-model:input="draft"
        input-test-id="chat-peer-input"
        send-test-id="chat-peer-send"
        :placeholder="t('chat.peer.placeholder')"
        :send-disabled="!canSend"
        @send="submit"
        @transcript="onTranscript"
      >
        <template #above>
          <p
            v-if="conversation.peerWithoutRelays"
            class="mb-2 flex items-start gap-1 text-xs text-muted-foreground"
            data-testid="chat-peer-relay-hint"
          >
            <Icon name="lucide:info" class="mt-0.5 h-3 w-3 shrink-0" />
            {{ t('chat.peer.ownRelaysInstead') }}
          </p>
        </template>
        <template #below>
          <p
            v-if="length > MAX_TEXT_CHARS - 500"
            class="mt-1 text-right text-xs"
            :class="tooLong ? 'text-destructive' : 'text-muted-foreground'"
            data-testid="chat-peer-counter"
          >
            {{ t('chat.peer.counter', { count: length, max: MAX_TEXT_CHARS }) }}
          </p>
        </template>
      </ChatComposerShell>
    </template>
    <p v-else class="m-auto text-sm text-muted-foreground">
      {{ t('chat.peer.missing') }}
    </p>

    <UiDrawerModal
      v-if="conversation && confirmDelete"
      :open="confirmDelete"
      :title="t('chat.peer.delete')"
      @update:open="confirmDelete = $event"
    >
      <template #content>
        <p class="px-6 py-2 text-sm">
          {{ t('chat.peer.deleteConfirm', { name: conversation.displayName }) }}
        </p>
      </template>
      <template #footer>
        <div class="flex justify-end gap-2">
          <UiButton
            type="button"
            variant="outline"
            @click="confirmDelete = false"
          >
            {{ t('chat.cancel') }}
          </UiButton>
          <UiButton
            type="button"
            variant="destructive"
            data-testid="chat-peer-delete-confirm"
            @click="deleteConversation"
          >
            {{ t('chat.peer.deleteButton') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>
  </section>
</template>
