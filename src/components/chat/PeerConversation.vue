<script setup lang="ts">
/**
 * A contact or group conversation (spec 072 US1, US2, FR-006): header with picture and name, the
 * messages and a composer for text, without any model, effort, permission or agent control.
 * Sending and sending again run their catalog actions (`chat.peer.send`, `chat.peer.resend`);
 * the message appears at once as _waiting_ and moves on as the delivery goes.
 */
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
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const draft = ref('')
const confirmDelete = ref(false)
const sending = ref(false)
const sendError = ref<string | null>(null)
/** One key per message: a retry of the same draft sends it once (contracts/ipc.md). */
let idempotencyKey = crypto.randomUUID()

const threadId = computed(() => props.conversation?.id ?? null)
const peer = usePeerConversation(threadId, errString)

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
  sending.value = true
  const error = await props.send(id, draft.value, idempotencyKey)
  sending.value = false
  sendError.value = error
  if (error === null && threadId.value === id) {
    draft.value = ''
    idempotencyKey = crypto.randomUUID()
  }
}

function onKeydown(event: KeyboardEvent) {
  // action-exempt: Enter submits the draft; Shift+Enter starts a new line.
  if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
    event.preventDefault()
    void submit()
  }
}

async function resendMessage(messageId: string) {
  sendError.value = await props.resend(messageId)
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
    sendError.value = null
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
        <!-- Notes never notify, so they have nothing to mute (FR-002b). -->
        <UiButton
          v-if="conversation.kind !== 'note'"
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

      <div class="border-t border-border px-4 py-3 md:px-8">
        <p
          v-if="conversation.peerWithoutRelays"
          class="mb-2 flex items-start gap-1 text-xs text-muted-foreground"
          data-testid="chat-peer-relay-hint"
        >
          <Icon name="lucide:info" class="mt-0.5 h-3 w-3 shrink-0" />
          {{ t('chat.peer.ownRelaysInstead') }}
        </p>
        <p
          v-if="sendError ?? peer.error.value"
          class="mb-2 text-xs text-destructive"
          role="alert"
          data-testid="chat-peer-error"
        >
          {{ sendError ?? peer.error.value }}
        </p>
        <form class="flex items-end gap-2" @submit.prevent="submit">
          <UiTextarea
            v-model="draft"
            class="flex-1"
            :placeholder="t('chat.peer.placeholder')"
            :aria-label="t('chat.peer.placeholder')"
            rows="2"
            data-testid="chat-peer-input"
            @keydown="onKeydown"
          />
          <UiButton
            type="submit"
            size="icon"
            :disabled="!canSend"
            :aria-label="t('chat.peer.send')"
            :tooltip="t('chat.peer.send')"
            data-testid="chat-peer-send"
          >
            <Icon name="lucide:send" class="h-4 w-4" />
          </UiButton>
        </form>
        <p
          v-if="length > MAX_TEXT_CHARS - 500"
          class="mt-1 text-right text-xs"
          :class="tooLong ? 'text-destructive' : 'text-muted-foreground'"
          data-testid="chat-peer-counter"
        >
          {{ t('chat.peer.counter', { count: length, max: MAX_TEXT_CHARS }) }}
        </p>
      </div>
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
