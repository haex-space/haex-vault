import type { Ref } from 'vue'
import type {
  Message,
  MessageCompleteEvent,
  MessageErrorEvent,
  RetryEvent,
  TokenEvent,
  ToolCallEvent,
  ToolPermissionRequestEvent,
  ToolResultEvent,
  TurnCompleteEvent,
  useChat,
} from '~/composables/useChat'
import type { ChoiceRequestEvent, PendingPrompt } from '~/lib/chat/prompts'

type PendingStreamEvents = {
  tokens: string
  reasoning: string
  retryReset?: boolean
  complete?: MessageCompleteEvent
  error?: MessageErrorEvent
}

/**
 * Transcript state and the streaming/tool-call/turn-completion event
 * handlers that keep it consistent — extracted from
 * `src/pages/chat/[instance].vue` (2026-09-15 review, split step 2/4). See
 * that file's own history for the rest of the page's split plan.
 *
 * The three "pending" maps buffer events that arrive before their target
 * is ready to receive them: a token/complete/error for a message not yet
 * the active stream (`pendingStreamEvents`), a tool call/result for a
 * thread not yet active or still mid-`send()` (`pendingToolEvents`), and
 * a turn-complete that lands while `send()` is still seeding placeholders
 * (`pendingTurnCompletions`, drained by `send()` itself). `turnTerminalWaiters`
 * is unrelated to those three but travels with them since
 * `waitForTurnTerminal`/`resolveTurnTerminal` are its only accessors —
 * `hasPendingTurn`/`waitForTurnTerminal` are called by the thread sidebar's
 * `confirmDelete` (a cross-composable call, not a layering mistake).
 */
export function useChatTranscript(
  chat: ReturnType<typeof useChat>,
  messagesByThread: Ref<Record<string, Message[]>>,
  activeThreadId: Ref<string | null>,
  streamingMessageId: Ref<string | null>,
  streamingThreadId: Ref<string | null>,
  streamingBuffer: Ref<string>,
  reasoningByMessage: Ref<Record<string, string>>,
  retryingMessageId: Ref<string | null>,
  busy: Ref<boolean>,
  turnSetupPending: Ref<boolean>,
  lastError: Ref<string | null>,
  pendingPrompts: Ref<PendingPrompt[]>,
  pendingPromptsByThread: Map<string, PendingPrompt[]>,
  refreshThreads: () => Promise<void>,
  scrollToBottom: () => Promise<void>,
  errString: (e: unknown) => string,
) {
  const pendingStreamEvents = new Map<string, PendingStreamEvents>()
  const pendingToolEvents = new Map<
    string,
    Array<ToolCallEvent | ToolResultEvent>
  >()
  const pendingTurnCompletions = new Map<string, TurnCompleteEvent>()
  const turnTerminalWaiters = new Map<string, Set<() => void>>()

  function hasPendingTurn(threadId: string): boolean {
    return (
      streamingThreadId.value === threadId ||
      (turnSetupPending.value && activeThreadId.value === threadId) ||
      pendingPromptsByThread.has(threadId) ||
      (activeThreadId.value === threadId && pendingPrompts.value.length > 0)
    )
  }

  function waitForTurnTerminal(threadId: string): Promise<boolean> {
    if (!hasPendingTurn(threadId)) return Promise.resolve(true)
    return new Promise((resolve) => {
      const waiters = turnTerminalWaiters.get(threadId) ?? new Set<() => void>()
      const timeout = setTimeout(() => {
        waiters.delete(finish)
        if (waiters.size === 0) turnTerminalWaiters.delete(threadId)
        resolve(false)
      }, 10_000)
      const finish = () => {
        clearTimeout(timeout)
        waiters.delete(finish)
        if (waiters.size === 0) turnTerminalWaiters.delete(threadId)
        resolve(true)
      }
      waiters.add(finish)
      turnTerminalWaiters.set(threadId, waiters)
    })
  }

  function resolveTurnTerminal(threadId: string) {
    const waiters = turnTerminalWaiters.get(threadId)
    if (!waiters) return
    for (const waiter of [...waiters]) waiter()
  }

  function pendingFor(messageId: string): PendingStreamEvents {
    const existing = pendingStreamEvents.get(messageId)
    if (existing) return existing
    const pending: PendingStreamEvents = { tokens: '', reasoning: '' }
    pendingStreamEvents.set(messageId, pending)
    return pending
  }

  function applyToken(e: TokenEvent, threadId: string) {
    if (retryingMessageId.value === e.messageId) retryingMessageId.value = null
    if (e.delta) {
      streamingBuffer.value += e.delta
      const list = messagesByThread.value[threadId] ?? []
      const idx = list.findIndex((m) => m.id === e.messageId)
      const existing = list[idx]
      if (idx !== -1 && existing) {
        list[idx] = { ...existing, content: existing.content + e.delta }
        messagesByThread.value[threadId] = list
      }
    }
    if (e.reasoning) {
      const prev = reasoningByMessage.value[e.messageId] ?? ''
      reasoningByMessage.value = {
        ...reasoningByMessage.value,
        [e.messageId]: prev + e.reasoning,
      }
    }
    scrollToBottom()
  }

  function handleToken(e: TokenEvent) {
    const threadId = streamingThreadId.value
    if (streamingMessageId.value !== e.messageId || !threadId) {
      const pending = pendingFor(e.messageId)
      if (e.delta) pending.tokens += e.delta
      if (e.reasoning) pending.reasoning += e.reasoning
      return
    }
    applyToken(e, threadId)
  }

  /** `chat-retry`: the attempt whose partial text was already shown just
   * got discarded — clear it and show "retrying…" instead (T039). Preserve
   * the reset when the event lands before `send()` installs its placeholder. */
  function handleRetry(e: RetryEvent) {
    const threadId = streamingThreadId.value
    if (streamingMessageId.value !== e.assistantMessageId || !threadId) {
      const pending = pendingFor(e.assistantMessageId)
      pending.tokens = ''
      pending.reasoning = ''
      pending.retryReset = true
      return
    }
    streamingBuffer.value = ''
    reasoningByMessage.value = {
      ...reasoningByMessage.value,
      [e.assistantMessageId]: '',
    }
    const list = messagesByThread.value[threadId] ?? []
    const idx = list.findIndex((m) => m.id === e.assistantMessageId)
    const existing = list[idx]
    if (idx !== -1 && existing) {
      list[idx] = { ...existing, content: '' }
      messagesByThread.value[threadId] = list
    }
    retryingMessageId.value = e.assistantMessageId
  }

  // `chat-message-complete` fires per step (a turn can have several); it no
  // longer clears `streamingMessageId`/`busy` or sets a terminal
  // `finishReason` itself — `chat-turn-complete` is the sole source for
  // both, since only it knows whether the turn actually ended in
  // `complete` vs. `tool_limit_reached` (contracts/tauri-commands.md).
  function applyComplete(e: MessageCompleteEvent) {
    const list = messagesByThread.value[e.threadId] ?? []
    const idx = list.findIndex((m) => m.id === e.messageId)
    const existing = list[idx]
    if (idx !== -1 && existing) {
      list[idx] = {
        ...existing,
        promptTokens: e.promptTokens,
        completionTokens: e.completionTokens,
      }
      messagesByThread.value[e.threadId] = list
    }
  }

  function handleComplete(e: MessageCompleteEvent) {
    if (streamingMessageId.value !== e.messageId) {
      const pending = pendingFor(e.messageId)
      pending.complete = e
      delete pending.error
      return
    }
    applyComplete(e)
  }

  function applyError(e: MessageErrorEvent) {
    // A failure with its own kind is named in the person's language (spec 043: an untrusted
    // certificate); every other one shows its reason.
    lastError.value =
      e.kind === undefined ? e.reason : errString({ kind: e.kind })
    const list = messagesByThread.value[e.threadId] ?? []
    const idx = list.findIndex((m) => m.id === e.messageId)
    const existing = list[idx]
    if (idx !== -1 && existing) {
      list[idx] = { ...existing, finishReason: 'error' }
      messagesByThread.value[e.threadId] = list
    }
  }

  function handleError(e: MessageErrorEvent) {
    if (streamingMessageId.value !== e.messageId) {
      const pending = pendingFor(e.messageId)
      pending.error = e
      delete pending.complete
      return
    }
    applyError(e)
  }

  /** Appends a `tool_call`/`tool_result` row. Only applied once the event's
   * own thread is the one currently open AND that thread's turn placeholder
   * rows are in place — unlike token/complete/error events, these carry no
   * pre-known placeholder to buffer against, so while `send()` is still
   * setting one up (`turnSetupPending`) an event for the now-active thread
   * would otherwise render above the user message that triggered it. The
   * row is safely in `chat_messages` regardless; switching back to that
   * thread reloads it via `selectThread`. */
  function handleToolCall(e: ToolCallEvent, restoring = false) {
    if (
      !restoring &&
      (e.threadId !== activeThreadId.value || turnSetupPending.value)
    ) {
      const queued = pendingToolEvents.get(e.threadId) ?? []
      pendingToolEvents.set(e.threadId, [...queued, e])
      return
    }
    const list = messagesByThread.value[e.threadId] ?? []
    if (list.some((message) => message.id === e.messageId)) return
    list.push({
      id: e.messageId,
      threadId: e.threadId,
      parentId: null,
      role: 'tool_call',
      content: '',
      modelId: null,
      promptTokens: null,
      completionTokens: null,
      finishReason: null,
      createdAt: Date.now(),
      toolName: e.toolName,
      toolCallId: null,
      toolInput: JSON.stringify(e.toolInput),
      toolIsError: null,
      toolSource: e.toolSource,
      toolOrigin: e.toolOrigin,
      autonomyMode: null,
    })
    messagesByThread.value[e.threadId] = list
    scrollToBottom()
  }

  function handleToolResult(e: ToolResultEvent, restoring = false) {
    if (
      !restoring &&
      (e.threadId !== activeThreadId.value || turnSetupPending.value)
    ) {
      const queued = pendingToolEvents.get(e.threadId) ?? []
      pendingToolEvents.set(e.threadId, [...queued, e])
      return
    }
    const list = messagesByThread.value[e.threadId] ?? []
    if (list.some((message) => message.id === e.messageId)) return
    list.push({
      id: e.messageId,
      threadId: e.threadId,
      parentId: null,
      role: 'tool_result',
      content: e.content,
      modelId: null,
      promptTokens: null,
      completionTokens: null,
      finishReason: null,
      createdAt: Date.now(),
      toolName: null,
      toolCallId: e.toolCallId,
      toolInput: null,
      toolIsError: e.isError,
      toolSource: null,
      autonomyMode: null,
    })
    messagesByThread.value[e.threadId] = list
    scrollToBottom()
  }

  // The sole place `streamingMessageId`/`busy` get cleared — a turn can
  // span several steps, so only its one terminal event may signal "done"
  // (contracts/tauri-commands.md).
  async function applyTurnComplete(e: TurnCompleteEvent) {
    pendingPromptsByThread.delete(e.threadId)
    if (activeThreadId.value === e.threadId) pendingPrompts.value = []
    const list = messagesByThread.value[e.threadId] ?? []
    const idx = list.findIndex((m) => m.id === e.assistantMessageId)
    const existing = list[idx]
    if (idx !== -1 && existing) {
      list[idx] = { ...existing, finishReason: e.finishReason }
      messagesByThread.value[e.threadId] = list
    }
    try {
      // All rounds stream through one placeholder, whereas persistence has
      // separate interim answers, tool rows, and the terminal answer.
      messagesByThread.value[e.threadId] = await chat.listMessagesAsync(
        e.threadId,
      )
      pendingToolEvents.delete(e.threadId)
      await refreshThreads()
      await scrollToBottom()
    } catch (error: unknown) {
      lastError.value = errString(error)
    } finally {
      streamingMessageId.value = null
      streamingThreadId.value = null
      streamingBuffer.value = ''
      retryingMessageId.value = null
      busy.value = false
    }
  }

  /** Queues a prompt of `threadId`: shown at once for the active thread (each request once),
   * parked for another thread until it is opened. */
  function queuePrompt(threadId: string, prompt: PendingPrompt) {
    if (threadId !== activeThreadId.value) {
      const queued = pendingPromptsByThread.get(threadId) ?? []
      pendingPromptsByThread.set(threadId, [...queued, prompt])
      return
    }
    if (
      !pendingPrompts.value.some((item) => item.requestId === prompt.requestId)
    ) {
      pendingPrompts.value = [...pendingPrompts.value, prompt]
    }
  }

  function handleToolPermissionRequest(e: ToolPermissionRequestEvent) {
    queuePrompt(e.threadId, {
      kind: 'approval',
      requestId: e.requestId,
      toolName: e.toolName,
      toolInput: e.toolInput,
      riskClass: e.riskClass,
      toolSource: e.toolSource,
      toolOrigin: e.toolOrigin,
    })
  }

  /** A question of the agent (spec 046): queued like an approval, in the same order. */
  function handleChoiceRequest(e: ChoiceRequestEvent) {
    const { threadId, ...request } = e
    queuePrompt(threadId, { kind: 'choice', ...request })
  }

  async function handleTurnComplete(e: TurnCompleteEvent) {
    if (turnSetupPending.value) {
      pendingTurnCompletions.set(e.threadId, e)
      return
    }
    if (e.threadId !== streamingThreadId.value) return
    if (
      e.assistantMessageId &&
      streamingMessageId.value !== e.assistantMessageId
    )
      return
    await applyTurnComplete(e)
    resolveTurnTerminal(e.threadId)
  }

  return {
    pendingStreamEvents,
    pendingToolEvents,
    pendingTurnCompletions,
    turnTerminalWaiters,
    pendingFor,
    applyToken,
    handleToken,
    handleRetry,
    applyComplete,
    handleComplete,
    applyError,
    handleError,
    handleToolCall,
    handleToolResult,
    applyTurnComplete,
    handleToolPermissionRequest,
    handleChoiceRequest,
    handleTurnComplete,
    resolveTurnTerminal,
    waitForTurnTerminal,
    hasPendingTurn,
  }
}
