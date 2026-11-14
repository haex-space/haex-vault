<script setup lang="ts">
/*
 * Orchestration of the chat app (specs 015, 072): shared state, model store, permission mode,
 * listeners and the app contract (`useWmTab`); the parts live in composables and components.
 */
import {
  computed,
  onMounted,
  onBeforeUnmount,
  ref,
  nextTick,
  useTemplateRef,
  watch,
} from 'vue'
import type { UnlistenFn } from '@tauri-apps/api/event'
import type { ChatSearchHit } from '@bindings/ChatSearchHit'
import type { Message, SendMessageArgs } from '~/composables/useChat'
import type { PendingPrompt } from '~/lib/chat/prompts'
import { promptSubscriptions } from '~/composables/useChatChoices'
import { followsAfterScroll } from '~/lib/chat/scrollFollow'

const wm = useWindowManagerStore()
const openSettings = useAction('wm.app.open')
const wmTab = useWmTab()
const { t } = useI18n()
const chat = useChat()
const { getPrefAsync, setPrefAsync } = usePreferences()
const { currentDeviceInfoAsync } = useDevice()
const { errString } = useErrorString()

const pendingPrompts = ref<PendingPrompt[]>([])
const unlisteners: UnlistenFn[] = []
let unmounted = false

const messagesByThread = ref<Record<string, Message[]>>({})
const activeThreadId = ref<string | null>(null)

const streamingMessageId = ref<string | null>(null)
const streamingThreadId = ref<string | null>(null)
const streamingBuffer = ref<string>('')
const reasoningByMessage = ref<Record<string, string>>({})
// Set between attempts of an automatic LLM-request retry (FR-012); never persisted.
const retryingMessageId = ref<string | null>(null)
const expandedReasoning = ref<Set<string>>(new Set())

const pendingPromptsByThread = new Map<string, PendingPrompt[]>()

const input = ref('')
/** True while the voice control is recording: it then provides the send button. */
const voiceRecording = ref(false)
const busy = ref(false)
// True from send() until its placeholder rows land — guards a same-thread event arriving first.
const turnSetupPending = ref(false)
const lastError = ref<string | null>(null)
const pendingSend = ref<SendMessageArgs | null>(null)

const {
  permissionMode,
  deviceUuid,
  permissionModeSaving,
  updatePermissionMode,
  initialize: initializePermissionMode,
} = useChatPermissionMode(getPrefAsync, setPrefAsync, errString, lastError)

const {
  modelStore,
  activeModel,
  modelLoadPending,
  loadingModelName,
  loadErrorModelId,
  loadingLabel,
  displayModelId,
  effortLevel,
  providerList,
  integrityDialog,
  integrityBusy,
  integrityActionError,
  onIntegrityDialogOpenChange,
  retryLoad,
  decideIntegrity,
  isDelegateModel,
  composerInputDisabled,
  sendDisabled,
  displayedError,
  dismissError,
  showModelSelection,
} = useAgentModelState({
  busy,
  streamingMessageId,
  lastError,
})

// Spec 072: contact and group conversations share the list and the window with the agent's.
const peers = useConversations(errString)
const { activePeerId } = peers
const newContactOpen = ref(false)

const { attachments, addAttachments, addPasted, removeAttachment } =
  useComposerAttachments(chat, displayModelId, errString, lastError)

// Composer.vue owns the <textarea>/auto-resize; wrapped as the plain `() => Promise<void>` useComposer/useThreadSidebar expect.
const composerRef = useTemplateRef<{ reset: () => Promise<void> } | null>(
  'composer',
)
const messagesScroll = useTemplateRef<HTMLElement>('messagesScroll')
const resetTextarea = async () => composerRef.value?.reset()

// chatTranscript/threadSidebar need each other's functions — resolved via a placeholder swapped for the real refreshThreads once threadSidebar exists.
let refreshThreadsForTranscript = async () => {}
const chatTranscript = useChatTranscript(
  chat,
  messagesByThread,
  activeThreadId,
  streamingMessageId,
  streamingThreadId,
  streamingBuffer,
  reasoningByMessage,
  retryingMessageId,
  busy,
  turnSetupPending,
  lastError,
  pendingPrompts,
  pendingPromptsByThread,
  () => refreshThreadsForTranscript(),
  scrollToBottom,
  errString,
)
const threadSidebar = useThreadSidebar(
  chat,
  chatTranscript,
  t,
  messagesByThread,
  activeThreadId,
  input,
  streamingMessageId,
  streamingThreadId,
  streamingBuffer,
  reasoningByMessage,
  expandedReasoning,
  pendingPrompts,
  pendingPromptsByThread,
  resetTextarea,
  scrollToBottom,
)
refreshThreadsForTranscript = threadSidebar.refreshThreads

const {
  turnTerminalWaiters,
  handleToken,
  handleRetry,
  handleComplete,
  handleError,
  handleToolCall,
  handleToolResult,
  handleTurnComplete,
} = chatTranscript

const {
  threads,
  editingThreadId,
  draftTitle,
  editTitleError,
  renameError,
  renamingThreadId,
  deleteCandidate,
  deleteError,
  deletingThread,
  // Unused here — kept because check-chat-state.ts replays it standalone.
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  historyDuration,
  historyDurationLabel,
  openingTimeLabel,
  stopDurationRefresh,
  refreshThreads,
  refreshLoadedMessages,
  startEditing,
  cancelEditing,
  saveThreadTitle,
  requestDelete,
  closeDeleteDialog,
  confirmDelete,
  selectThread,
} = threadSidebar

const activeMessages = computed<Message[]>(() => {
  if (!activeThreadId.value) return []
  return messagesByThread.value[activeThreadId.value] ?? []
})

// Spec 066 T127: a spoken session can show its transcript instead of its messages. A search hit
// in "Verlauf" opens the session with the transcript at that entry; another session starts with
// its messages.
const transcriptOpen = ref(false)
// `request` counts the selections, so choosing the same hit again scrolls to it again.
const transcriptFocus = ref<{ id: string; request: number } | null>(null)
let focusRequests = 0
let pendingTranscript: { threadId: string; id: string } | null = null
const activeIsVoice = computed(
  () =>
    threads.value.find((thread) => thread.id === activeThreadId.value)
      ?.origin === 'voice',
)
watch(activeThreadId, (threadId) => {
  const pending = pendingTranscript
  pendingTranscript = null
  const hit = pending?.threadId === threadId ? pending : null
  transcriptOpen.value = hit !== null
  transcriptFocus.value = hit ? { id: hit.id, request: ++focusRequests } : null
})
// Closing the transcript returns to the newest message.
watch(transcriptOpen, (isOpen) => {
  if (!isOpen) void scrollToBottom({ force: true })
})
function openTranscript(hit: { threadId: string; id: string }) {
  if (hit.threadId === activeThreadId.value) {
    transcriptOpen.value = true
    transcriptFocus.value = { id: hit.id, request: ++focusRequests }
    return
  }
  pendingTranscript = hit
  void ui.openConversation(hit.threadId)
}

/** Whether the viewport follows new content: true while the reader is at its end, false once
 * they scrolled up to read earlier messages. */
let followsBottom = true
/** The viewport's `scrollTop` at the previous scroll event, to tell a move up from a move down. */
let lastScrollTop = 0

function onMessagesScroll() {
  const el = messagesScroll.value
  if (!el) return
  followsBottom = followsAfterScroll(followsBottom, lastScrollTop, el)
  lastScrollTop = el.scrollTop
}

/**
 * Scrolls the message viewport to its newest item after rendering, but only while the reader is
 * at the end: a reply that is being written must not pull them away from what they scrolled up to
 * read. `force` is for what the reader just did themselves, sending a message or opening a thread.
 */
async function scrollToBottom({ force = false }: { force?: boolean } = {}) {
  // The transcript shares this viewport; it must stay where a search hit scrolled it.
  if (transcriptOpen.value && activeIsVoice.value) return
  if (!force && !followsBottom) return
  await nextTick()
  const el = messagesScroll.value
  if (!el) return
  // The reader may have scrolled up while the render was pending.
  if (!force && !followsBottom) return
  el.scrollTop = el.scrollHeight
  followsBottom = true
}

const {
  send,
  abort,
  newChat,
  setReasoningExpanded,
  onVoiceTranscript,
  activeAgentCount,
  lastAgentBatchSize,
  handleAgentActivity,
  resetAgentActivity,
} = useComposer(
  chat,
  chatTranscript,
  refreshThreads,
  scrollToBottom,
  errString,
  resetTextarea,
  input,
  busy,
  pendingSend,
  turnSetupPending,
  lastError,
  activeModel,
  modelLoadPending,
  isDelegateModel,
  effortLevel,
  sendDisabled,
  activeThreadId,
  messagesByThread,
  streamingMessageId,
  streamingThreadId,
  streamingBuffer,
  reasoningByMessage,
  retryingMessageId,
  expandedReasoning,
  attachments,
  pendingPrompts,
  pendingPromptsByThread,
)

// Spec 032 US4: the once-per-conversation notice about a model's tool use.
const {
  toolNotice,
  handleToolAvailability,
  handleProfileRestricted,
  handleCompaction,
  dismissToolNotice,
} = useToolNotice(activeThreadId)

// Spec 083 US5: the notice about an agent profile opens that profile.
function openProfile(profileId: string) {
  void openSettings({
    appId: 'system.settings',
    at: `/ai/profiles/${profileId}`,
  })
}

// Spec 020: tab history, tab-bound chat actions, approval response and close guard (useChatTab).
const { syncFromLocation, ui } = useChatTab({
  wmTab,
  router: useTabRouter(),
  runAction: wm.runAction,
  chat,
  errString,
  newChatLabel: () => t('chat.newChat'),
  state: {
    activeThreadId,
    threads,
    input,
    pendingPrompts,
    lastError,
    streamingMessageId,
    turnSetupPending,
    editingThreadId,
    draftTitle,
    editTitleError,
    deleteCandidate,
    deleteError,
  },
  selectThread,
  saveThreadTitle,
  confirmDelete,
  send,
  abort,
  newChat,
  updatePermissionMode,
  peer: { ...peers, parkShare: useShareInboxStore().park },
})

/** A message hit of the sidebar's search (spec 076 FR-012): its conversation, for the agent the
 * session holding the message. */
function openHit(hit: ChatSearchHit) {
  if (hit.kind === 'agent') ui.openConversation(hit.threadId)
  else ui.openPeerConversation(hit.threadId)
}

// Spec 024 (FR-032): a thread or message created, renamed or deleted by another device or window
// must appear here without reloading. While a turn runs here its own events and its turn-complete
// reload keep the transcript current, so messages are left alone until it is over.
onVaultTablesChanged(['chat_threads'], async () => {
  try {
    await refreshThreads()
  } catch (e: unknown) {
    if (!unmounted) lastError.value = errString(e)
  }
})
onVaultTablesChanged(['chat_messages'], async () => {
  if (turnSetupPending.value) return
  try {
    // The active thread is left alone while its local turn runs; other loaded threads update.
    await refreshLoadedMessages(busy.value ? activeThreadId.value : undefined)
  } catch (e: unknown) {
    if (!unmounted) lastError.value = errString(e)
  }
})

onMounted(async () => {
  try {
    await Promise.all([
      registerChatSubscriptions(
        [
          chat.onToken(handleToken),
          chat.onMessageComplete(handleComplete),
          chat.onMessageError(handleError),
          chat.onToolCall(handleToolCall),
          chat.onToolResult(handleToolResult),
          chat.onRetry(handleRetry),
          chat.onAgentActivity(handleAgentActivity),
          chat.onTurnComplete((e) => {
            // A turn ending clears agent-activity state (FR-011, same signal that already clears
            // streamingMessageId/busy) and tab attention (R12) alike.
            resetAgentActivity()
            wmTab.clearAttention()
            return handleTurnComplete(e)
          }),
          chat.onToolAvailability(handleToolAvailability),
          chat.onProfileRestricted(handleProfileRestricted),
          chat.onCompaction(handleCompaction),
          ...promptSubscriptions(chat, wmTab, chatTranscript),
        ],
        unlisteners,
        () => unmounted,
      ),
      modelStore.startListening(),
    ])
    if (unmounted) return

    const device = await currentDeviceInfoAsync()
    await initializePermissionMode(device.vaultDeviceUuid)
    if (unmounted) return

    await modelStore.initialize()
    await Promise.all([refreshThreads(), peers.refresh()])
    await syncFromLocation()
  } catch (e: unknown) {
    if (!unmounted) lastError.value = errString(e)
  }
})

onBeforeUnmount(() => {
  unmounted = true
  modelStore.stopListening()
  stopDurationRefresh()
  turnTerminalWaiters.clear()
  if (streamingMessageId.value || turnSetupPending.value) void abort() // can't reconstruct approvals on remount
  for (const unlisten of unlisteners.splice(0)) unlisten()
})
</script>

<template>
  <ChatSidebarLayout @new-conversation="newContactOpen = true">
    <template #sidebar="{ run }">
      <ChatThreadSidebar
        :threads="threads"
        :agent-active="activePeerId === null"
        :peers="peers.conversations.value"
        :notes="peers.notes.value"
        :active-peer-id="activePeerId"
        @select-agent="run(ui.openAgent)"
        @select-notes="run(ui.openNotes)"
        @select-peer="(id) => run(() => ui.openPeerConversation(id))"
        @select-hit="(hit) => run(() => openHit(hit))"
      />
    </template>

    <ChatPeerConversation
      v-if="activePeerId !== null"
      :conversation="peers.active.value"
      :send="ui.sendPeer"
      :resend="ui.resendPeer"
      @mute="ui.mute"
      @delete="ui.deletePeer"
      @answer="ui.answerRequest"
    />

    <section v-else class="min-h-0 min-w-0 flex-1 flex flex-col">
      <ChatAgentHeader
        v-model:transcript="transcriptOpen"
        :threads="threads"
        :active-thread-id="activeThreadId"
        :busy="busy"
        @new-session="ui.newConversation"
      >
        <ChatAgentSessionsPopover
          :threads="threads"
          :active-thread-id="activeThreadId"
          :editing-thread-id="editingThreadId"
          :draft-title="draftTitle"
          :edit-title-error="editTitleError"
          :rename-error="renameError"
          :renaming-thread-id="renamingThreadId"
          :delete-candidate="deleteCandidate"
          :delete-error="deleteError"
          :deleting-thread="deletingThread"
          :history-duration-label="historyDurationLabel"
          :opening-time-label="openingTimeLabel"
          @update:draft-title="draftTitle = $event"
          @select="ui.openConversation"
          @start-editing="startEditing"
          @save-title="ui.saveTitle"
          @cancel-editing="cancelEditing"
          @request-delete="requestDelete"
          @close-delete-dialog="closeDeleteDialog"
          @confirm-delete="ui.confirmDelete"
          @open-transcript="openTranscript"
        />
      </ChatAgentHeader>
      <ChatStatusBanners
        :displayed-error="displayedError"
        :can-retry-send="!!pendingSend"
        :load-error-model-id="loadErrorModelId"
        :loading-label="loadingLabel"
        :tool-notice="toolNotice"
        @dismiss-tool-notice="dismissToolNotice"
        @open-profile="openProfile"
        @retry-send="ui.retrySend"
        @retry-model-load="retryLoad()"
        @dismiss-error="dismissError"
      />

      <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
        <div
          ref="messagesScroll"
          data-messages-scroll
          class="flex-1 min-h-0 overflow-y-auto px-4 py-6 md:px-8"
          @scroll.passive="onMessagesScroll"
        >
          <ChatVoiceTranscript
            v-if="transcriptOpen && activeIsVoice && activeThreadId"
            :thread-id="activeThreadId"
            :focus="transcriptFocus"
          />
          <ChatMessageList
            v-else
            :active-messages="activeMessages"
            :show-model-selection="showModelSelection"
            :active-model-name="activeModel?.name"
            :loading-model-name="loadingModelName"
            :streaming-message-id="streamingMessageId"
            :retrying-message-id="retryingMessageId"
            :reasoning-by-message="reasoningByMessage"
            :expanded-reasoning="expandedReasoning"
            :provider-list="providerList"
            @toggle-reasoning="setReasoningExpanded"
          />
        </div>

        <ChatComposer
          ref="composer"
          v-model:input="input"
          v-model:voice-recording="voiceRecording"
          :composer-input-disabled="composerInputDisabled"
          :send-disabled="sendDisabled"
          :streaming-message-id="streamingMessageId"
          :turn-setup-pending="turnSetupPending"
          :busy="busy"
          :attachments="attachments"
          :active-agent-count="activeAgentCount"
          :last-agent-batch-size="lastAgentBatchSize"
          :permission-mode="permissionMode"
          :pending-prompts="pendingPrompts"
          :thread-id="activeThreadId"
          :permission-disabled="!deviceUuid || permissionModeSaving"
          @send="ui.send"
          @abort="ui.abort"
          @add-attachments="addAttachments"
          @paste-files="addPasted"
          @remove-attachment="removeAttachment"
          @update-permission-mode="ui.setPermissionMode"
          @respond-approval="ui.respondApproval"
          @answer-choice="ui.answerChoice"
          @transcript="onVoiceTranscript"
        />
      </div>
    </section>

    <template #dialogs>
      <ChatNewContactDialog
        v-model:open="newContactOpen"
        :contacts="peers.contacts.value"
        @select="
          (id) => {
            newContactOpen = false
            ui.openDirect(id)
          }
        "
      />
      <ModelsModelIntegrityDialog
        v-if="integrityDialog"
        :open="integrityDialog !== null"
        :error-kind="integrityDialog.errorKind"
        :expected-sha256="integrityDialog.expected"
        :actual-sha256="integrityDialog.actual"
        :busy="integrityBusy"
        :action-error="integrityActionError"
        @update:open="onIntegrityDialogOpenChange"
        @load-untrusted="decideIntegrity({ decision: 'loadUntrusted' })"
        @repair-source="decideIntegrity({ decision: 'repairSource' })"
        @choose-other="decideIntegrity({ decision: 'chooseOther' })"
      />
    </template>
  </ChatSidebarLayout>
</template>
