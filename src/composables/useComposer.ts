import { ref, type Ref } from 'vue'
import type {
  AgentActivityEvent,
  LoadedModelInfo,
  Message,
  SendMessageArgs,
  useChat,
} from '~/composables/useChat'
import type { PendingPrompt } from '~/lib/chat/prompts'
import type { useChatTranscript } from '~/composables/useChatTranscript'
import type { ComposerAttachment } from '~/components/chat/ComposerAttachments.vue'

/**
 * Composer send/cancel/new-conversation flow — extracted from
 * `src/pages/chat/[instance].vue` (spec 015-workspace-shell, T006, plan
 * research R10). `input`, `busy`, `pendingSend` and `turnSetupPending`
 * stay refs the page creates and shares with `useChatTranscript`/
 * `useThreadSidebar` (the same convention those two composables already
 * use for `messagesByThread`/`activeThreadId`/etc.) — this composable
 * receives them rather than creating them, so the three composables can
 * be built in any order without a new circular-dependency workaround.
 */
export function useComposer(
  chat: ReturnType<typeof useChat>,
  chatTranscript: ReturnType<typeof useChatTranscript>,
  refreshThreads: () => Promise<void>,
  scrollToBottom: (options?: { force?: boolean }) => Promise<void>,
  errString: (e: unknown) => string,
  resetTextarea: () => Promise<void>,
  input: Ref<string>,
  busy: Ref<boolean>,
  pendingSend: Ref<SendMessageArgs | null>,
  turnSetupPending: Ref<boolean>,
  lastError: Ref<string | null>,
  activeModel: Ref<LoadedModelInfo | null>,
  modelLoadPending: Ref<boolean>,
  isDelegateModel: Ref<boolean>,
  effortLevel: Ref<string | null>,
  sendDisabled: Ref<boolean>,
  activeThreadId: Ref<string | null>,
  messagesByThread: Ref<Record<string, Message[]>>,
  streamingMessageId: Ref<string | null>,
  streamingThreadId: Ref<string | null>,
  streamingBuffer: Ref<string>,
  reasoningByMessage: Ref<Record<string, string>>,
  retryingMessageId: Ref<string | null>,
  expandedReasoning: Ref<Set<string>>,
  attachments: Ref<ComposerAttachment[]>,
  pendingPrompts: Ref<PendingPrompt[]>,
  pendingPromptsByThread: Map<string, PendingPrompt[]>,
) {
  let sendGeneration = 0
  let sendInvocationStarted = false

  // Live sub-agent activity for the Claude Code delegate (spec
  // 011-composer-toolbar-parity Story 2) — reset at the start of every send
  // and when the current turn ends, so a stale count never survives into
  // the next turn.
  const activeAgentCount = ref(0)
  const lastAgentBatchSize = ref<number | null>(null)

  function handleAgentActivity(e: AgentActivityEvent) {
    activeAgentCount.value = e.activeCount
    if (e.batchSize !== undefined) lastAgentBatchSize.value = e.batchSize
  }

  function resetAgentActivity() {
    activeAgentCount.value = 0
    lastAgentBatchSize.value = null
  }

  const {
    pendingStreamEvents,
    pendingToolEvents,
    pendingTurnCompletions,
    handleToken,
    handleComplete,
    handleError,
    handleToolCall,
    handleToolResult,
    handleTurnComplete,
  } = chatTranscript

  /** Sends the current input and seeds local message placeholders for streaming. */
  async function send(retryPending = false) {
    const retry = retryPending ? pendingSend.value : null
    const content = retry?.content ?? input.value.trim()
    if (!content || sendDisabled.value) return
    const generation = ++sendGeneration
    sendInvocationStarted = false
    if (!retry) input.value = ''
    busy.value = true
    lastError.value = null
    activeAgentCount.value = 0
    lastAgentBatchSize.value = null
    const request = retry ?? {
      threadId: activeThreadId.value,
      content,
      idempotencyKey: crypto.randomUUID(),
      reasoningOption: effortLevel.value,
      attachments: attachments.value.map((a) => ({ path: a.path })),
    }
    pendingSend.value = null
    if (!retry) attachments.value = []
    turnSetupPending.value = true
    try {
      // A thread selection starts this load in the background. Await it again here so a fast
      // send cannot reserve the chat operation first and use the previously loaded model.
      await chat.useSessionModelAsync(request.threadId ?? null)
      if (generation !== sendGeneration) return
      sendInvocationStarted = true
      const result = await chat.sendMessageAsync(request)
      activeThreadId.value = result.threadId
      const queuedApprovals = pendingPromptsByThread.get(result.threadId)
      if (queuedApprovals) {
        // Merge, don't overwrite: see the matching comment in `selectThread`.
        pendingPrompts.value = [...queuedApprovals, ...pendingPrompts.value]
        pendingPromptsByThread.delete(result.threadId)
      }
      if (retry) {
        // A replay returns the original IDs without emitting another turn.
        // Recover a completed answer when the initial invoke response was lost.
        const persisted = await chat.listMessagesAsync(result.threadId)
        messagesByThread.value[result.threadId] = persisted
        const completed = persisted.find(
          (message) => message.id === result.assistantMessageId,
        )
        if (completed?.finishReason) {
          streamingMessageId.value = result.assistantMessageId
          streamingThreadId.value = result.threadId
          turnSetupPending.value = false
          pendingStreamEvents.delete(result.assistantMessageId)
          pendingTurnCompletions.delete(result.threadId)
          await handleTurnComplete({
            threadId: result.threadId,
            assistantMessageId: result.assistantMessageId,
            finishReason: completed.finishReason,
          })
          return
        }
      }
      // Seed the assistant message placeholder so the UI can show
      // tokens as they stream in.
      const list = messagesByThread.value[result.threadId] ?? []
      if (!list.some((message) => message.id === result.userMessageId))
        list.push({
          id: result.userMessageId,
          threadId: result.threadId,
          parentId: null,
          role: 'user',
          content,
          modelId: null,
          promptTokens: null,
          completionTokens: null,
          finishReason: 'complete',
          createdAt: Date.now(),
          toolName: null,
          toolCallId: null,
          toolInput: null,
          toolIsError: null,
          toolSource: null,
          autonomyMode: null,
        })
      if (!list.some((message) => message.id === result.assistantMessageId))
        list.push({
          id: result.assistantMessageId,
          threadId: result.threadId,
          parentId: result.userMessageId,
          role: 'assistant',
          content: '',
          modelId: activeModel.value?.modelId ?? null,
          promptTokens: null,
          completionTokens: null,
          finishReason: null,
          createdAt: Date.now(),
          toolName: null,
          toolCallId: null,
          toolInput: null,
          toolIsError: null,
          toolSource: null,
          autonomyMode: null,
        })
      messagesByThread.value[result.threadId] = list
      turnSetupPending.value = false
      streamingMessageId.value = result.assistantMessageId
      streamingThreadId.value = result.threadId
      streamingBuffer.value = ''
      retryingMessageId.value = null
      reasoningByMessage.value = {
        ...reasoningByMessage.value,
        [result.assistantMessageId]: '',
      }
      const queuedToolEvents = pendingToolEvents.get(result.threadId)
      if (queuedToolEvents) {
        pendingToolEvents.delete(result.threadId)
        for (const event of queuedToolEvents) {
          if ('toolName' in event) handleToolCall(event, true)
          else handleToolResult(event, true)
        }
      }
      const pending = pendingStreamEvents.get(result.assistantMessageId)
      pendingStreamEvents.delete(result.assistantMessageId)
      if (pending?.retryReset) {
        streamingBuffer.value = ''
        reasoningByMessage.value = {
          ...reasoningByMessage.value,
          [result.assistantMessageId]: '',
        }
      }
      if (pending?.tokens || pending?.reasoning) {
        handleToken({
          messageId: result.assistantMessageId,
          delta: pending?.tokens ?? '',
          reasoning: pending?.reasoning ? pending.reasoning : null,
        })
      }
      if (pending?.error) {
        handleError(pending.error)
      } else if (pending?.complete) {
        handleComplete(pending.complete)
      }
      const completedTurn = pendingTurnCompletions.get(result.threadId)
      pendingTurnCompletions.delete(result.threadId)
      if (completedTurn) {
        await handleTurnComplete(completedTurn)
      } else {
        // A failed list refresh must not turn an accepted send into a retry.
        await refreshThreads().catch((e: unknown) => {
          lastError.value = errString(e)
        })
      }
      // The reader just sent this message: show it, wherever they had scrolled to.
      await scrollToBottom({ force: true })
    } catch (e: unknown) {
      if (generation !== sendGeneration) return
      // The initial invoke may have been accepted even when its response was
      // lost. Keep the exact same key available for a safe retry; stream
      // errors are handled separately and deliberately do not retry.
      pendingSend.value = request
      lastError.value = errString(e)
      busy.value = false
      turnSetupPending.value = false
    }
  }

  /**
   * Writes a dictated transcript into the composer (FR-004) and, when
   * auto-send is on, sends it immediately through the same path as manually
   * typed input (FR-005) — same gating as pressing Enter, so it silently
   * stays in the field if e.g. no model is loaded yet.
   */
  function onVoiceTranscript(text: string, autoSend: boolean) {
    input.value = text
    if (autoSend) void send()
  }

  /** Requests cancellation of the active generation. */
  async function abort() {
    if (turnSetupPending.value && !sendInvocationStarted) {
      // The session model is still loading; there is no chat generation to abort yet. Prevent the
      // pending preflight from starting a send after the page has been left.
      sendGeneration++
      busy.value = false
      turnSetupPending.value = false
      return
    }
    try {
      await chat.abortAsync()
    } catch (e: unknown) {
      lastError.value = errString(e)
    }
  }

  /** Clears the active conversation so the next send creates a new thread. */
  async function newChat() {
    if (busy.value || modelLoadPending.value) return
    if (activeThreadId.value && pendingPrompts.value.length > 0) {
      const queued = pendingPromptsByThread.get(activeThreadId.value) ?? []
      pendingPromptsByThread.set(activeThreadId.value, [
        ...queued,
        ...pendingPrompts.value,
      ])
      pendingPrompts.value = []
    }
    activeThreadId.value = null
    // A new session starts with the last model picked in the model menu (spec 006).
    void chat.useSessionModelAsync(null).catch(() => {})
    input.value = ''
    attachments.value = []
    streamingMessageId.value = null
    streamingThreadId.value = null
    streamingBuffer.value = ''
    reasoningByMessage.value = {}
    expandedReasoning.value = new Set()
    pendingStreamEvents.clear()
    void resetTextarea()
  }

  /** Expands or collapses one message's reasoning accordion. */
  function setReasoningExpanded(messageId: string, expanded: boolean) {
    const next = new Set(expandedReasoning.value)
    if (expanded) next.add(messageId)
    else next.delete(messageId)
    expandedReasoning.value = next
  }

  return {
    send,
    abort,
    newChat,
    setReasoningExpanded,
    onVoiceTranscript,
    activeAgentCount,
    lastAgentBatchSize,
    handleAgentActivity,
    resetAgentActivity,
  }
}
