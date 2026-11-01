<script setup lang="ts">
/*
 * Split across `useChatTranscript`/`useThreadSidebar`/`useComposer`/
 * `useComposerAttachments` (composables) and `ThreadSidebar`/`MessageList`/
 * `Composer` (components, spec 015-workspace-shell T010-T013) — see their
 * own headers. What's left is orchestration: shared page-level state,
 * model-store wiring, permission/autonomy-mode persistence, `onMounted`
 * listener setup, and the window manager-app contract (`useWmTab`, T032).
 *
 * Moved from `pages/chat/[instance].vue` into a window manager app (T021): instance
 * name from `useInstancesStore()` (no route of its own anymore). Onboarding enforcement moved to
 * the window manager host page (T025).
 */
import {
  computed,
  onMounted,
  onBeforeUnmount,
  ref,
  nextTick,
  useTemplateRef,
} from 'vue'
import type { UnlistenFn } from '@tauri-apps/api/event'
import type { Message, SendMessageArgs } from '~/composables/useChat'
import type { PendingPrompt } from '~/lib/chat/prompts'
import { promptSubscriptions } from '~/composables/useChatChoices'

const instancesStore = useInstancesStore()
const wm = useWindowManagerStore()
const wmTab = useWmTab()
const { t } = useI18n()
const chat = useChat()
const { getPrefAsync, setPrefAsync } = usePreferences()
const { currentDeviceInfoAsync } = useDevice()
const { errString } = useErrorString()

const pendingPrompts = ref<PendingPrompt[]>([])
const unlisteners: UnlistenFn[] = []
let unmounted = false

const instanceName = computed(() => instancesStore.activeInstance ?? '')

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
  autonomyMode,
  autonomyPreferenceLoading,
  autonomyPreferenceError,
  deviceUuid,
  permissionModeSaving,
  updatePermissionMode,
  reloadAutonomyMode,
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
  autonomyPreferenceLoading,
  autonomyPreferenceError,
})

// Spec 072: contact and group conversations share the list and the window with the agent's.
const peers = useConversations(errString)
const activePeerId = ref<string | null>(null)
const activePeer = computed(
  () =>
    peers.conversations.value.find((c) => c.id === activePeerId.value) ?? null,
)
const newContactOpen = ref(false)

const { attachments, addAttachments, removeAttachment } =
  useComposerAttachments(chat, displayModelId, errString, lastError)

// Composer.vue owns the <textarea>/auto-resize; wrapped as the plain `() => Promise<void>` useComposer/useThreadSidebar expect.
const composerRef = useTemplateRef<{ reset: () => Promise<void> } | null>(
  'composer',
)
const messagesScroll = useTemplateRef<HTMLElement>('messagesScroll')
async function resetTextarea() {
  await composerRef.value?.reset()
}

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

/** Scrolls the message viewport to its newest item after rendering. */
async function scrollToBottom() {
  await nextTick()
  const el = messagesScroll.value
  if (el) el.scrollTop = el.scrollHeight
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
  autonomyMode,
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
  dismissToolNotice,
  setDelegateRightsAsync,
} = useToolNotice(activeThreadId)

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
  peer: { ...peers, activePeerId },
})

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
    // Keep the active thread untouched while its local turn runs, but do not let that
    // suppress updates for other already-loaded threads.
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
  <ChatSidebarLayout>
    <template #sidebar="{ run }">
      <ChatThreadSidebar
        :instance-name="instanceName"
        :busy="busy"
        :threads="threads"
        :active-thread-id="activeThreadId"
        :editing-thread-id="editingThreadId"
        :draft-title="draftTitle"
        :edit-title-error="editTitleError"
        :renaming-thread-id="renamingThreadId"
        :delete-candidate="deleteCandidate"
        :delete-error="deleteError"
        :deleting-thread="deletingThread"
        :history-duration-label="historyDurationLabel"
        :opening-time-label="openingTimeLabel"
        :peers="peers.conversations.value"
        :active-peer-id="activePeerId"
        @update:draft-title="draftTitle = $event"
        @new-chat="run(ui.newConversation)"
        @select-thread="(id) => run(() => ui.openConversation(id))"
        @start-editing="startEditing"
        @save-title="ui.saveTitle"
        @cancel-editing="cancelEditing"
        @request-delete="requestDelete"
        @close-delete-dialog="closeDeleteDialog"
        @confirm-delete="ui.confirmDelete"
        @select-peer="(id) => run(() => ui.openPeerConversation(id))"
        @write-contact="newContactOpen = true"
      />
    </template>

    <ChatPeerConversation
      v-if="activePeerId !== null"
      :conversation="activePeer"
      @mute="ui.mute"
      @delete="ui.deletePeer"
    />

    <section v-else class="min-h-0 min-w-0 flex-1 flex flex-col">
      <ChatStatusBanners
        :displayed-error="displayedError"
        :can-retry-send="!!pendingSend"
        :load-error-model-id="loadErrorModelId"
        :autonomy-preference-error="autonomyPreferenceError"
        :autonomy-preference-loading="autonomyPreferenceLoading"
        :loading-label="loadingLabel"
        :tool-notice="toolNotice"
        @dismiss-tool-notice="dismissToolNotice"
        @set-delegate-rights="setDelegateRightsAsync"
        @retry-send="ui.retrySend"
        @retry-model-load="retryLoad()"
        @dismiss-error="dismissError"
        @retry-autonomy-mode="reloadAutonomyMode()"
      />

      <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
        <div
          ref="messagesScroll"
          data-messages-scroll
          class="flex-1 min-h-0 overflow-y-auto px-4 py-6 md:px-8"
        >
          <ChatMessageList
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
