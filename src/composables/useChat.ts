/*
 * Chat IPC wrappers and the wire/event types they exchange.
 *
 * Maintainability exception (spaex 500-LoC rule): the interface
 * declarations below mirror the backend's command and event payloads one to
 * one, and the `useChat()` wrappers are thin `invoke`/`listen` calls over
 * them, so the file grew past 500 lines without gaining unrelated
 * responsibilities. Concrete split plan, if this grows further: move the
 * wire/event interface declarations (`Thread` through `ModelLoadErrorEvent`)
 * into a type-only `useChatTypes.ts` re-exported from here, leaving the
 * command and event wrappers in `useChat()`.
 */
import type { RiskClass } from '~/lib/chat/prompts'
import type { ToolAvailability } from '~/lib/chat/toolNotice'
import type { ToolOrigin } from '~/lib/chat/toolOrigin'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export interface Thread {
  id: string
  title: string
  lastModelId: string | null
  createdAt: number
  updatedAt: number
}

export interface Message {
  id: string
  threadId: string
  parentId: string | null
  role: 'user' | 'assistant' | 'system' | 'tool_call' | 'tool_result'
  content: string
  modelId: string | null
  promptTokens: number | null
  completionTokens: number | null
  finishReason: 'complete' | 'cancelled' | 'error' | 'tool_limit_reached' | null
  createdAt: number
  /** Set only when `role === 'tool_call'` (data-model.md). */
  toolName: string | null
  /** Correlates a `tool_call` row with its `tool_result` row. */
  toolCallId: string | null
  /** JSON text of the tool input. Set only when `role === 'tool_call'`. */
  toolInput: string | null
  /** Set only when `role === 'tool_result'`. */
  toolIsError: boolean | null
  /** `mcp`, `cli`, `action` or `haextension`. Set only when `role === 'tool_call'`. */
  toolSource: string | null
  /** An extension's tool call (spec 018); absent otherwise. */
  toolOrigin?: ToolOrigin
  /** Which autonomy mode the turn ran under (spec
   * 009-autonomous-delegate-mode). `null` for legacy and non-delegate
   * rows — never inferred as `'standard'`. */
  autonomyMode: 'standard' | 'ungated' | 'gated_permissive' | null
}

export interface LoadedModelInfo {
  modelId: string
  name: string
  tokenizerRepo: string
  contextWindow: number | null
  /** The local engine running the model (`llama.cpp` or `mistralrs`, spec 070); null for a remote one. */
  engine: string | null
}

export interface SendMessageArgs {
  threadId?: string | null
  content: string
  systemPrompt?: string
  maxNewTokens?: number
  /**
   * Stable across every retry of the same send. Defaults to a fresh
   * `crypto.randomUUID()` when omitted — pass the same value back in
   * on a frontend retry (e.g. after a network error) so the backend
   * recognizes it and returns the original result instead of creating
   * a second user message (contract §send_message).
   */
  idempotencyKey?: string
  /**
   * Per-request autonomy posture for a `cli_delegate` backend (spec
   * 009-autonomous-delegate-mode). Omitted or `null` behaves identically
   * to `'standard'` — never persisted as a preference (FR-008), and
   * ignored entirely by non-delegate backends.
   */
  autonomyMode?: 'standard' | 'ungated' | 'gated_permissive' | null
  /**
   * The selected reasoning option as a provider-native id (spec 012).
   * `null`/omitted means Auto — the model's own default applies. The backend
   * validates it against the model's cached options and drops one that is no
   * longer offered. Never persisted with the message.
   */
  reasoningOption?: string | null
  /** Files attached to this message (spec 011-composer-toolbar-parity),
   * identified by the path the file picker returned. Scoped to this one
   * send (FR-017) — never carried over to a later message. */
  attachments?: AttachmentInput[]
}

export interface SendMessageResult {
  threadId: string
  userMessageId: string
  assistantMessageId: string
  /** The key used for this send; reuse it when retrying the same invoke. */
  idempotencyKey: string
  /** Names of attachments that could not be read at send time and were
   * excluded (FR-018) — the rest of the message still sent. */
  excludedAttachments: string[]
}

export interface AttachmentInput {
  /** The choice of the open dialog (spec 043: a path or an address). */
  path: string
}

export type AttachmentKind = 'image' | 'document' | 'text'

/** Result of `inspect_attachment` (spec 011-composer-toolbar-parity,
 * contracts/tauri-commands.md) — called right after the file picker
 * resolves a path, before the file is staged in the composer. */
export interface AttachmentInfo {
  name: string
  sizeBytes: number
  /** `null` when the file's extension isn't a supported attachment type
   * at all (`usable` is then always `false`). */
  kind: AttachmentKind | null
  usable: boolean
  reason?: string
}

export interface TokenEvent {
  messageId: string
  delta: string
  /**
   * Reasoning-content delta (chain-of-thought for Harmony-format local
   * models or Anthropic `thinking_delta` events). `null` for chunks
   * that carry only regular content.
   */
  reasoning: string | null
}

export interface MessageCompleteEvent {
  messageId: string
  threadId: string
  promptTokens: number | null
  completionTokens: number | null
  ttftMs: number | null
}

export interface MessageErrorEvent {
  messageId: string
  threadId: string
  reason: string
  /** Set for a failure the chat names itself (spec 043 FR-024). */
  kind?: 'UntrustedCertificate'
}

export interface ToolCallEvent {
  messageId: string
  threadId: string
  toolName: string
  toolInput: unknown
  toolSource: 'mcp' | 'cli' | 'action' | 'haextension'
  toolOrigin?: ToolOrigin
}

export interface ToolResultEvent {
  messageId: string
  threadId: string
  toolCallId: string
  content: string
  isError: boolean
}

/** Fires while a Claude Code delegate response has sub-agents running
 * (spec 011-composer-toolbar-parity) — never emitted by any other backend.
 * `batchSize` is present only on the update where a new batch of that many
 * sub-agents was just confirmed dispatched. */
export interface AgentActivityEvent {
  messageId: string
  activeCount: number
  batchSize?: number
}

/** Fires when the Manual/Auto/Plan gate needs a human decision (contracts/
 * tauri-commands.md). Answered via `respondToolPermissionAsync`. */
export interface ToolPermissionRequestEvent {
  requestId: string
  threadId: string
  toolName: string
  toolInput: unknown
  riskClass: RiskClass
  toolSource: 'mcp' | 'cli' | 'action' | 'haextension'
  toolOrigin?: ToolOrigin
}

/** Fires once per `send_message` call: whether the model got tools (spec 032 US4). */
export interface ToolAvailabilityEvent {
  threadId: string
  state: ToolAvailability
}

/** Fires exactly once per `send_message` call, after that turn's last
 * per-step event — the sole signal to clear `streamingMessageId`/`busy`,
 * since a turn can span multiple steps (contracts/tauri-commands.md). */
export interface TurnCompleteEvent {
  threadId: string
  assistantMessageId: string | null
  finishReason: 'complete' | 'cancelled' | 'error' | 'tool_limit_reached'
}

/** Fires on each automatic retry attempt (spec.md FR-012/FR-013) — transient,
 * never persisted to `chat_messages`. The frontend should clear whatever
 * partial text it had already shown for `assistantMessageId` and show a
 * "retrying…" indicator instead, since that text belongs to the discarded,
 * now-being-retried attempt. */
export interface RetryEvent {
  threadId: string
  assistantMessageId: string
  /** 1-based. */
  attempt: number
}

export type ModelLoadPhase =
  'connecting' | 'loading' | 'cuda-jit-warmup' | 'ready'

export interface ModelLoadStatusIdle {
  status: 'idle'
  vaultGeneration: number
}

export interface ModelLoadStatusLoading {
  status: 'loading'
  vaultGeneration: number
  loadId: number
  modelId: string
  modelName: string
  phase: Exclude<ModelLoadPhase, 'ready'>
  providerName?: string
}

export interface ModelLoadStatusReady {
  status: 'ready'
  vaultGeneration: number
  loadId: number
  modelId: string
  modelName: string
}

export interface ModelLoadStatusError {
  status: 'error'
  vaultGeneration: number
  loadId: number
  modelId?: string
  modelName?: string
  code: string
}

export type ModelLoadStatusPayload =
  | ModelLoadStatusIdle
  | ModelLoadStatusLoading
  | ModelLoadStatusReady
  | ModelLoadStatusError

export interface ModelLoadProgressEvent {
  modelId: string
  modelName: string
  phase: ModelLoadPhase
  vaultGeneration: number
  loadId: number
  /** Present only when `phase === 'connecting'` (spec 002 §FR-015b). */
  providerName?: string
}

export interface ModelLoadErrorEvent {
  vaultGeneration: number
  loadId: number
  modelId?: string
  modelName?: string
  code: string
}

/**
 * Chat runtime: thread + message CRUD, model load/unload, streaming
 * send with three companion Tauri events (`chat-token`,
 * `chat-message-complete`, `chat-message-error`).
 */
export function useChat() {
  let latestVaultGeneration: number | null = null
  let latestLoadId = -1

  function acceptsModelLoadEvent(event: {
    vaultGeneration: number
    loadId: number
  }): boolean {
    if (
      latestVaultGeneration === null ||
      event.vaultGeneration > latestVaultGeneration
    ) {
      latestVaultGeneration = event.vaultGeneration
      latestLoadId = event.loadId
      return true
    }
    if (
      event.vaultGeneration < latestVaultGeneration ||
      event.loadId < latestLoadId
    ) {
      return false
    }
    latestLoadId = event.loadId
    return true
  }

  /** Lists chat threads for the active instance. */
  async function listThreadsAsync(): Promise<Thread[]> {
    return await invoke<Thread[]>('list_threads')
  }

  /** Lists persisted messages in chronological order for one thread. */
  async function listMessagesAsync(threadId: string): Promise<Message[]> {
    return await invoke<Message[]>('list_messages', { threadId })
  }

  /** Creates and returns an empty chat thread. */
  async function createThreadAsync(title?: string): Promise<Thread> {
    return await invoke<Thread>('create_thread', {
      args: { title: title ?? null },
    })
  }

  /** Persists a trimmed thread title and returns the refreshed thread. */
  async function renameThreadAsync(
    threadId: string,
    title: string,
  ): Promise<Thread> {
    return await invoke<Thread>('rename_thread', {
      args: { threadId, title },
    })
  }

  /** Deletes one persisted thread and all of its messages. */
  async function deleteThreadAsync(threadId: string): Promise<void> {
    await invoke('delete_thread', { args: { threadId } })
  }

  /** Persists a user turn and starts streaming the assistant response. */
  async function sendMessageAsync(
    args: SendMessageArgs,
  ): Promise<SendMessageResult> {
    const idempotencyKey = args.idempotencyKey ?? crypto.randomUUID()
    const result = await invoke<Omit<SendMessageResult, 'idempotencyKey'>>(
      'send_message',
      {
        args: { ...args, idempotencyKey },
      },
    )
    return { ...result, idempotencyKey }
  }

  /** Aborts the active generation, if one is running. */
  async function abortAsync(): Promise<void> {
    await invoke('abort_current_generation')
  }

  /**
   * Classifies a file the user is about to attach and reports whether the
   * given model/backend can actually use it (contracts/tauri-commands.md
   * `inspect_attachment`). Called right after the file picker resolves a
   * path, before it's added to the composer's attachment list.
   */
  async function inspectAttachmentAsync(
    path: string,
    modelId: string,
  ): Promise<AttachmentInfo> {
    return await invoke<AttachmentInfo>('inspect_attachment', { path, modelId })
  }

  /** Resolves an open `tool-permission-request` (contracts §respond_tool_permission). */
  async function respondToolPermissionAsync(
    requestId: string,
    decision: 'allow' | 'deny',
  ): Promise<void> {
    await invoke('respond_tool_permission', {
      args: { requestId, decision },
    })
  }

  /**
   * Loads a model into the chat session. Accepts either a local
   * catalog id (`"llama-3.1-8b"`) or a composite api_key id
   * (`"<provider_uuid>:claude-opus-5"`); the backend routes on the
   * colon.
   */
  async function loadModelAsync(modelId: string): Promise<LoadedModelInfo> {
    return await invoke<LoadedModelInfo>('load_model', { modelId })
  }

  /**
   * Explicit, confirmation-gated bypass of the pre-load integrity check
   * (spec 005 §"load_model und lokale Integritätsprüfung") — the
   * `load_untrusted` action of the integrity dialog. Marks the model
   * `untrusted` and loads the file currently on disk as-is; never touches
   * the expected `fileSha256`.
   */
  async function loadModelWithIntegrityOverrideAsync(
    modelId: string,
  ): Promise<LoadedModelInfo> {
    return await invoke<LoadedModelInfo>('load_model_with_integrity_override', {
      modelId,
    })
  }

  /** Unloads the model currently held by the chat session. */
  async function unloadModelAsync(): Promise<void> {
    await invoke('unload_local_model')
  }

  /** Returns metadata for the loaded model, or `null` when none is loaded. */
  async function activeModelInfoAsync(): Promise<LoadedModelInfo | null> {
    return await invoke<LoadedModelInfo | null>('active_model_info')
  }

  /** Subscribes to streamed token deltas and returns the unlisten function. */
  async function onToken(
    handler: (e: TokenEvent) => void,
  ): Promise<UnlistenFn> {
    return await listen<TokenEvent>('chat-token', (ev) => handler(ev.payload))
  }

  /** Subscribes to successful generation completions and returns the unlisten function. */
  async function onMessageComplete(
    handler: (e: MessageCompleteEvent) => void,
  ): Promise<UnlistenFn> {
    return await listen<MessageCompleteEvent>('chat-message-complete', (ev) =>
      handler(ev.payload),
    )
  }

  /** Subscribes to failed generations and returns the unlisten function. */
  async function onMessageError(
    handler: (e: MessageErrorEvent) => void,
  ): Promise<UnlistenFn> {
    return await listen<MessageErrorEvent>('chat-message-error', (ev) =>
      handler(ev.payload),
    )
  }

  /** Subscribes to persisted tool calls and returns the unlisten function. */
  async function onToolCall(
    handler: (e: ToolCallEvent) => void,
  ): Promise<UnlistenFn> {
    return await listen<ToolCallEvent>('chat-tool-call', (ev) =>
      handler(ev.payload),
    )
  }

  /** Subscribes to persisted tool results and returns the unlisten function. */
  async function onToolResult(
    handler: (e: ToolResultEvent) => void,
  ): Promise<UnlistenFn> {
    return await listen<ToolResultEvent>('chat-tool-result', (ev) =>
      handler(ev.payload),
    )
  }

  /**
   * Subscribes to the turn's terminal event and returns the unlisten
   * function. This — not `chat-message-complete`/`chat-message-error` — is
   * the signal to clear `streamingMessageId`/`busy`, since a turn can span
   * multiple steps (contracts/tauri-commands.md).
   */
  async function onTurnComplete(
    handler: (e: TurnCompleteEvent) => void,
  ): Promise<UnlistenFn> {
    return await listen<TurnCompleteEvent>('chat-turn-complete', (ev) =>
      handler(ev.payload),
    )
  }

  /** Subscribes to `chat-agent-activity` and returns the unlisten function. */
  async function onAgentActivity(
    handler: (e: AgentActivityEvent) => void,
  ): Promise<UnlistenFn> {
    return await listen<AgentActivityEvent>('chat-agent-activity', (ev) =>
      handler(ev.payload),
    )
  }

  /** Subscribes to `chat-retry` and returns the unlisten function. */
  async function onRetry(
    handler: (e: RetryEvent) => void,
  ): Promise<UnlistenFn> {
    return await listen<RetryEvent>('chat-retry', (ev) => handler(ev.payload))
  }

  /** Subscribes to `tool-permission-request` and returns the unlisten function. */
  async function onToolPermissionRequest(
    handler: (e: ToolPermissionRequestEvent) => void,
  ): Promise<UnlistenFn> {
    return await listen<ToolPermissionRequestEvent>(
      'tool-permission-request',
      (ev) => handler(ev.payload),
    )
  }

  /** Subscribes to `chat-tool-availability` and returns the unlisten function. */
  async function onToolAvailability(
    handler: (e: ToolAvailabilityEvent) => void,
  ): Promise<UnlistenFn> {
    return await listen<ToolAvailabilityEvent>('chat-tool-availability', (ev) =>
      handler(ev.payload),
    )
  }

  /**
   * Subscribes to `model-load-progress` and returns the unlisten
   * function. Payload carries the semantic phase + parameters; the
   * caller translates the label via `$t('chat.loading.<phase>', ...)`
   * (spec 002 §FR-020 i18n boundary).
   */
  async function onModelLoadProgress(
    handler: (e: ModelLoadProgressEvent) => void,
  ): Promise<UnlistenFn> {
    return await listen<ModelLoadProgressEvent>('model-load-progress', (ev) => {
      if (acceptsModelLoadEvent(ev.payload)) handler(ev.payload)
    })
  }

  /** Returns the current model-load state for the active Vault. */
  async function modelLoadStatusAsync(): Promise<ModelLoadStatusPayload | null> {
    const status = await invoke<ModelLoadStatusPayload>('model_load_status')
    const loadId = 'loadId' in status ? status.loadId : -1
    if (
      latestVaultGeneration !== null &&
      (status.vaultGeneration < latestVaultGeneration ||
        (status.vaultGeneration === latestVaultGeneration &&
          loadId < latestLoadId))
    )
      return null
    latestVaultGeneration = status.vaultGeneration
    latestLoadId = loadId
    return status
  }

  /** Subscribes to authoritative model-load snapshots emitted by lifecycle commands. */
  async function onModelLoadStatus(
    handler: (e: ModelLoadStatusPayload) => void,
  ): Promise<UnlistenFn> {
    return await listen<ModelLoadStatusPayload>('model-load-status', (ev) => {
      const payload = ev.payload
      const loadId = 'loadId' in payload ? payload.loadId : -1
      if (
        acceptsModelLoadEvent({
          vaultGeneration: payload.vaultGeneration,
          loadId,
        })
      ) {
        handler(payload)
      }
    })
  }

  /** Subscribes to structured model-load failures. */
  async function onModelLoadError(
    handler: (e: ModelLoadErrorEvent) => void,
  ): Promise<UnlistenFn> {
    return await listen<ModelLoadErrorEvent>('model-load-error', (ev) => {
      if (acceptsModelLoadEvent(ev.payload)) handler(ev.payload)
    })
  }

  return {
    listThreadsAsync,
    listMessagesAsync,
    createThreadAsync,
    renameThreadAsync,
    deleteThreadAsync,
    sendMessageAsync,
    abortAsync,
    inspectAttachmentAsync,
    respondToolPermissionAsync,
    loadModelAsync,
    loadModelWithIntegrityOverrideAsync,
    unloadModelAsync,
    activeModelInfoAsync,
    onToken,
    onMessageComplete,
    onMessageError,
    onToolCall,
    onToolResult,
    onAgentActivity,
    onRetry,
    onTurnComplete,
    onToolPermissionRequest,
    onToolAvailability,
    onModelLoadProgress,
    modelLoadStatusAsync,
    onModelLoadStatus,
    onModelLoadError,
  }
}
