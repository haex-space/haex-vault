<script setup lang="ts">
/**
 * A contact or group conversation (spec 072 US1, US2, FR-006): header with picture and name, the
 * messages and the agent's input field (`ComposerShell.vue`) with text and dictation, without any
 * model, effort, permission or agent control.
 * Sending and sending again run their catalog actions (`chat.peer.send`, `chat.peer.resend`);
 * the message appears at once as _waiting_ and moves on as the delivery goes. A request (US5) shows
 * its answers in place of the composer: nothing is sent to its sender before it is accepted.
 * The header names the conversation's protection state and explains it on demand (spec 095 US4).
 */
import { toast } from 'vue-sonner'
import type { PeerConversationEntry } from '~/composables/useConversations'
import type { PeerFile } from '~/components/chat/PeerFileChips.vue'
import {
  COUNTER_FROM,
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
    files: string[],
  ) => Promise<string | null>
  /** Runs `chat.peer.resend`; resolves with an error text when it failed. */
  resend: (messageId: string) => Promise<string | null>
}>()

const emit = defineEmits<{
  mute: [threadId: string, muted: boolean]
  /** The hint without a Blossom-Server leads to the chat settings (FR-038). */
  openChatSettings: []
  delete: [threadId: string]
  answer: [threadId: string, answer: 'accept' | 'decline' | 'block']
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const { pickManyAsync, nameOfAsync } = usePickedFile()
const { hasServer } = useChatFileServer()
const draft = ref('')
const files = ref<PeerFile[]>([])
const confirmDelete = ref(false)
/** The explanation of the protection state is open (spec 095 US4). */
const showProtection = ref(false)
const sending = ref(false)
const preparingFiles = ref(false)
/** Invalidates a pending picker when the user leaves and later returns to the same conversation. */
let fileSelectionGeneration = 0
/** One key per message: a retry of the same draft sends it once (contracts/ipc.md). */
let idempotencyKey = crypto.randomUUID()

const threadId = computed(() => props.conversation?.id ?? null)
const peer = usePeerConversation(threadId, errString)
// A failed load or read mark of the conversation shows as a toast.
watch(peer.error, (message) => {
  if (message) toast.error(message)
})

// Characters are at most UTF-16 units: only near the bound is the exact count worth its cost.
const length = computed(() =>
  draft.value.length < COUNTER_FROM
    ? draft.value.length
    : [...draft.value].length,
)
const tooLong = computed(() => length.value > MAX_TEXT_CHARS)
const canSend = computed(
  () =>
    !sending.value &&
    !preparingFiles.value &&
    !tooLong.value &&
    (draft.value.trim().length > 0 || files.value.length > 0) &&
    props.conversation !== null,
)
// Files go to the Blossom-Server, so a conversation offers them only with one (FR-038); notes stay
// in the vault and take no files.
const canAttach = computed(
  () => hasServer.value && props.conversation?.kind !== 'note',
)

async function attach() {
  const conversationId = threadId.value
  const generation = fileSelectionGeneration
  if (
    sending.value ||
    preparingFiles.value ||
    !conversationId ||
    !canAttach.value
  )
    return
  preparingFiles.value = true
  try {
    const picked = await pickManyAsync()
    if (
      fileSelectionGeneration !== generation ||
      threadId.value !== conversationId ||
      !canAttach.value
    )
      return
    const known = new Set(files.value.map((file) => file.path))
    for (const path of picked) {
      if (known.has(path)) continue
      const name = await nameOfAsync(path)
      if (
        fileSelectionGeneration !== generation ||
        threadId.value !== conversationId ||
        !canAttach.value
      )
        return
      files.value.push({ path, name })
    }
  } catch (error: unknown) {
    toast.error(errString(error))
  } finally {
    preparingFiles.value = false
  }
}

function detach(path: string) {
  files.value = files.value.filter((file) => file.path !== path)
}

async function submit() {
  const id = threadId.value
  if (!id || !canSend.value) return
  const text = draft.value
  const sent = files.value
  sending.value = true
  const error = await props.send(
    id,
    text,
    idempotencyKey,
    sent.map((file) => file.path),
  )
  sending.value = false
  if (error !== null) toast.error(error)
  if (threadId.value !== id) return
  // Text typed or dictated while the send ran is a new message: it stays in the field and gets
  // its own key, so neither a success clears it nor a retry sends it under the old key.
  const draftChanged = draft.value !== text
  if (error === null && !draftChanged) draft.value = ''
  if (error === null) files.value = []
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
    fileSelectionGeneration += 1
    draft.value = ''
    files.value = []
    idempotencyKey = crypto.randomUUID()
    confirmDelete.value = false
    showProtection.value = false
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
        <!-- Notes and conversations with a model have no protection state (spec 095 US4). -->
        <ChatProtectionState
          v-if="peer.protection.value"
          v-model:open="showProtection"
          :protection="peer.protection.value"
        />
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
      <ChatProtectionExplanation
        v-if="showProtection && peer.protection.value"
        :protection="peer.protection.value"
      />

      <div
        class="flex min-h-0 flex-1 flex-col-reverse overflow-y-auto px-4 py-6 md:px-8"
      >
        <ChatPeerMessageList
          v-if="peer.messages.value.length > 0"
          :messages="peer.messages.value"
          :peer-name="conversation.displayName"
          :peer-photo="conversation.photo"
          @resend="resendMessage"
          @download="peer.download"
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
        <template #attachments>
          <ChatPeerFileChips
            v-if="files.length > 0"
            :files="files"
            :sending="sending"
            @remove="detach"
          />
        </template>
        <template v-if="canAttach" #controls>
          <UiButton
            type="button"
            variant="ghost"
            size="icon"
            :disabled="sending || preparingFiles"
            :aria-label="t('chat.peer.attach')"
            :tooltip="t('chat.peer.attach')"
            data-testid="chat-peer-attach"
            @click="attach"
          >
            <Icon name="lucide:paperclip" class="h-4 w-4" />
          </UiButton>
        </template>
        <template #above>
          <p
            v-if="!hasServer && conversation.kind !== 'note'"
            class="mb-2 flex flex-wrap items-center gap-x-1 text-xs text-muted-foreground"
            data-testid="chat-peer-no-server-hint"
          >
            <Icon name="lucide:info" class="h-3 w-3 shrink-0" />
            {{ t('chat.peer.noFileServer') }}
            <button
              type="button"
              class="underline underline-offset-2 outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
              data-testid="chat-peer-open-chat-settings"
              @click="emit('openChatSettings')"
            >
              {{ t('chat.peer.openChatSettings') }}
            </button>
          </p>
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
            v-if="length >= COUNTER_FROM"
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
