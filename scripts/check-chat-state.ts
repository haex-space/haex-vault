// Run with `node scripts/check-chat-state.ts`. Replays the real page, its real composables and the
// real `useModelsStore` with Tauri replaced at its actual IPC boundary. No browser or GPU is
// required. The sandbox that boots the page lives in `scripts/lib/chat-state-harness.ts`; see that
// file for how it works and for what is faked.
//
// Maintainability exception (spaex 500-LoC rule): 45 replay tests in one file. Concrete split plan,
// if this grows further: split the cases by what they exercise — transcript/event ordering, thread
// sidebar, model lifecycle, and composer/permission state. New frontend cases for spec 013 go in
// `scripts/check-vault-lifecycle.ts`, not here.
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { nextTick } from 'vue'
import { createChatState, flush } from './lib/chat-state-harness.ts'

test('model download failures use the localized Hugging Face message', async () => {
  const state = createChatState(
    {},
    {},
    {
      useModels: () => ({
        listInstalledAsync: async () => [],
        onDownloadProgress: async () => () => {},
        downloadFromCatalogAsync: async () => {
          throw { kind: 'ModelDownload' }
        },
      }),
    },
  )

  await state.modelStore.downloadCatalogEntry({
    id: 'qwen3-0.6b',
    approx_size_bytes: 1,
  })

  assert.equal(state.modelStore.lastError, 'errors.hf.modelDownload')
})

test('model initialization clears stale transient UI errors', async () => {
  const state = createChatState()
  state.modelStore.lastError = 'stale error'
  state.modelStore.loadErrorModelId = 'stale-model'
  state.modelStore.downloadingId = 'download-in-progress'
  state.modelStore.integrityDialog = {
    modelId: 'stale-model',
    errorKind: 'HashMismatch',
    expected: 'expected',
    actual: 'actual',
  }
  state.modelStore.integrityActionError = 'stale action error'

  await state.modelStore.initialize()

  assert.equal(state.modelStore.lastError, null)
  assert.equal(state.modelStore.loadErrorModelId, null)
  assert.equal(state.modelStore.downloadingId, 'download-in-progress')
  assert.equal(state.modelStore.integrityDialog, null)
  assert.equal(state.modelStore.integrityActionError, null)
})

test('model initialization preserves an active integrity action', async () => {
  const state = createChatState()
  const dialog = {
    modelId: 'repairing-model',
    errorKind: 'HashMismatch',
    expected: 'expected',
    actual: 'actual',
  }
  state.modelStore.integrityDialog = dialog
  state.modelStore.integrityBusy = true
  state.modelStore.integrityActionError = 'action still running'

  await state.modelStore.initialize()

  assert.deepEqual(state.modelStore.integrityDialog, dialog)
  assert.equal(state.modelStore.integrityActionError, 'action still running')
})

test('model initialization restores the last picked model when nothing is active yet (spec 002 FR-014)', async () => {
  const state = createChatState(
    {
      activeLocalModelInfoAsync: async () => null,
      activeModelInfoAsync: async () => null,
      loadModelAsync: async (modelId: string) => ({
        modelId,
        name: 'Last Picked',
        tokenizerRepo: '',
        contextWindow: null,
      }),
    },
    {
      resolveDefaultModelAsync: async () => ({
        modelId: 'last-picked',
        source: 'last_active',
      }),
    },
  )

  await state.modelStore.initialize()

  assert.equal(state.modelStore.activeModel?.modelId, 'last-picked')
})

test('model initialization loads nothing without a picked model: there is no default (spec 002 FR-014)', async () => {
  let loadModelCalls = 0
  const state = createChatState({
    activeLocalModelInfoAsync: async () => null,
    activeModelInfoAsync: async () => null,
    loadModelAsync: async (modelId: string) => {
      loadModelCalls++
      return { modelId, name: modelId, tokenizerRepo: '', contextWindow: null }
    },
  })

  await state.modelStore.initialize()

  assert.equal(state.modelStore.activeModel, null)
  assert.equal(loadModelCalls, 0)
})

test('model initialization skips the auto-load fallback when a model is already active', async () => {
  let loadModelCalls = 0
  const state = createChatState({
    loadModelAsync: async (modelId: string) => {
      loadModelCalls++
      return { modelId, name: modelId, tokenizerRepo: '', contextWindow: null }
    },
  })

  await state.modelStore.initialize()

  assert.equal(state.modelStore.activeModel?.modelId, 'model')
  assert.equal(loadModelCalls, 0)
})

test('picking a model updates the composer display before load_model resolves', async () => {
  let resolveLoad: () => void = () => {}
  const state = createChatState({
    loadModelAsync: (modelId: string) =>
      new Promise((resolve) => {
        resolveLoad = () =>
          resolve({
            modelId,
            name: 'Qwen 3 4B',
            tokenizerRepo: '',
            contextWindow: null,
          })
      }),
  })
  state.modelStore.installedModels = [{ id: 'qwen3-4b', name: 'Qwen 3 4B' }]

  // `createChatState()` seeds a baseline active model ('model'/'Model') so
  // this starts from the realistic case: switching away from an already-
  // loaded model, not just picking a first one.
  const loadPromise = state.modelStore.loadModel('qwen3-4b')

  // Synchronous portion of `loadModel` (everything before its first
  // `await`) has already run by the time the call above returns — the
  // composer's display fields must reflect the pick immediately, without
  // waiting for `load_model`'s round trip. `activeModel` itself stays on
  // the previous model until the load actually resolves (`sendDisabled`
  // depends on that distinction).
  assert.equal(state.modelStore.displayModelId, 'qwen3-4b')
  assert.equal(state.modelStore.displayModelName, 'Qwen 3 4B')
  assert.equal(state.modelStore.activeModel?.modelId, 'model')

  resolveLoad()
  await loadPromise

  assert.equal(state.modelStore.activeModel?.modelId, 'qwen3-4b')
  assert.equal(state.modelStore.displayModelId, 'qwen3-4b')
  assert.equal(state.modelStore.displayModelName, 'Qwen 3 4B')
})

test('history durations use Unix milliseconds and compact thresholds', () => {
  const state = createChatState()
  const now = 10 * 86_400_000
  assert.deepEqual(state.historyDuration(now - 30_000, now), {
    value: 0,
    unit: 'min',
  })
  assert.deepEqual(state.historyDuration(now - 60_000, now), {
    value: 1,
    unit: 'min',
  })
  assert.deepEqual(state.historyDuration(now - 2 * 3_600_000, now), {
    value: 2,
    unit: 'h',
  })
  assert.deepEqual(state.historyDuration(now - 5 * 86_400_000, now), {
    value: 5,
    unit: 'd',
  })
})

test('history durations clamp future and unusable timestamps to zero minutes', () => {
  const state = createChatState()
  assert.deepEqual(state.historyDuration(Date.now() + 60_000), {
    value: 0,
    unit: 'min',
  })
  assert.deepEqual(state.historyDuration(-1), { value: 0, unit: 'min' })
  assert.deepEqual(state.historyDuration(Number.NaN), {
    value: 0,
    unit: 'min',
  })
})

test('renaming a history entry trims the title and updates only that row', async () => {
  const calls: { threadId: string; title: string }[] = []
  const state = createChatState({
    renameThreadAsync: async (threadId, title) => {
      calls.push({ threadId, title })
      return {
        id: threadId,
        title,
        lastModelId: null,
        createdAt: 1_000,
        updatedAt: 2_000,
      }
    },
  })
  state.threads.value = [
    {
      id: 'a',
      title: 'Old title',
      lastModelId: null,
      createdAt: 1_000,
      updatedAt: 2_000,
    },
    {
      id: 'b',
      title: 'Keep title',
      lastModelId: null,
      createdAt: 1_000,
      updatedAt: 2_000,
    },
  ]
  state.startEditing(state.threads.value[0])
  state.draftTitle.value = '  New title  '
  await state.saveThreadTitle()
  assert.deepEqual(calls, [{ threadId: 'a', title: 'New title' }])
  assert.equal(state.threads.value[0].title, 'New title')
  assert.equal(state.threads.value[1].title, 'Keep title')
  assert.equal(state.editingThreadId.value, null)
})

test('deleting the active history entry clears its cached session without selecting another', async () => {
  let deletedId = null
  const state = createChatState({
    deleteThreadAsync: async (threadId) => {
      deletedId = threadId
    },
  })
  state.threads.value = [
    {
      id: 'a',
      title: 'Active',
      lastModelId: null,
      createdAt: 1_000,
      updatedAt: 2_000,
    },
    {
      id: 'b',
      title: 'Other',
      lastModelId: null,
      createdAt: 1_000,
      updatedAt: 2_000,
    },
  ]
  state.activeThreadId.value = 'a'
  state.messagesByThread.value = { a: [{ id: 'message' }] }
  state.requestDelete(state.threads.value[0])
  await state.confirmDelete()
  assert.equal(deletedId, 'a')
  assert.deepEqual(
    state.threads.value.map((thread) => thread.id),
    ['b'],
  )
  assert.equal(state.activeThreadId.value, null)
  assert.equal(state.messagesByThread.value.a, undefined)
})

test('deleting a running active thread aborts and waits before persistence', async () => {
  const events: string[] = []
  const chat = {
    abortAsync: async () => {
      events.push('abort')
      await state.handleTurnComplete({
        threadId: 'a',
        assistantMessageId: 'answer',
        finishReason: 'cancelled',
      })
    },
    deleteThreadAsync: async () => {
      events.push('delete')
    },
  }
  const state = createChatState(chat)
  state.threads.value = [
    {
      id: 'a',
      title: 'Active',
      lastModelId: null,
      createdAt: 1_000,
      updatedAt: 2_000,
    },
  ]
  state.activeThreadId.value = 'a'
  state.streamingThreadId.value = 'a'
  state.streamingMessageId.value = 'answer'
  state.requestDelete(state.threads.value[0])
  await state.confirmDelete()
  assert.deepEqual(events, ['abort', 'delete'])
  assert.equal(state.threads.value.length, 0)
})

test('rename failures keep the previous title and edit mode available', async () => {
  const state = createChatState({
    renameThreadAsync: async () => {
      throw new Error('persist failed')
    },
  })
  state.threads.value = [
    {
      id: 'a',
      title: 'Keep me',
      lastModelId: null,
      createdAt: 1_000,
      updatedAt: 2_000,
    },
  ]
  state.startEditing(state.threads.value[0])
  state.draftTitle.value = 'New title'
  await state.saveThreadTitle()
  assert.equal(state.threads.value[0].title, 'Keep me')
  assert.equal(state.editingThreadId.value, 'a')
  assert.equal(state.renameError.value, 'chat.threads.renameFailed')
})

test('delete failures keep the history entry and its confirmation target', async () => {
  const state = createChatState({
    deleteThreadAsync: async () => {
      throw new Error('persist failed')
    },
  })
  state.threads.value = [
    {
      id: 'a',
      title: 'Keep me',
      lastModelId: null,
      createdAt: 1_000,
      updatedAt: 2_000,
    },
  ]
  state.requestDelete(state.threads.value[0])
  await state.confirmDelete()
  assert.equal(state.threads.value[0].title, 'Keep me')
  assert.equal(state.deleteCandidate.value.id, 'a')
  assert.equal(state.deleteError.value, 'chat.threads.deleteFailed')
})

test('an accepted first send appears in the conversation list immediately', async () => {
  const state = createChatState()
  state.input.value = 'Hello'
  await state.send()
  assert.deepEqual(
    state.threads.value.map((thread) => thread.id),
    ['a'],
  )
  assert.equal(state.busy.value, true)
})

test('keeps a draft editable while model loading blocks sending', async () => {
  let sends = 0
  const state = createChatState({
    sendMessageAsync: async () => {
      sends++
      return { threadId: 'a', userMessageId: 'u', assistantMessageId: 'answer' }
    },
  })
  state.input.value = 'Hello while loading'
  state.activeModel.value = null
  state.modelLoadPending.value = true
  state.busy.value = true

  assert.equal(state.composerInputDisabled.value, false)
  assert.equal(state.sendDisabled.value, true)
  await state.send()
  assert.equal(sends, 0)
  assert.equal(state.input.value, 'Hello while loading')

  state.modelLoadPending.value = false
  state.busy.value = false
  state.activeModel.value = {
    modelId: 'model',
    name: 'Model',
    tokenizerRepo: 'tokenizer',
    contextWindow: null,
  }
  assert.equal(state.sendDisabled.value, false)
  await state.send()
  assert.equal(sends, 1)
})

test('tokens and retries stay with the generating thread after switching conversations', async () => {
  const state = createChatState()
  state.input.value = 'Hello'
  await state.send()
  await state.selectThread('b')
  state.handleToken({
    messageId: 'answer',
    delta: 'Failed attempt',
    reasoning: null,
  })
  assert.equal(state.messagesByThread.value.a[1].content, 'Failed attempt')
  state.handleRetry({ threadId: 'a', assistantMessageId: 'answer', attempt: 1 })
  assert.equal(state.messagesByThread.value.a[1].content, '')
  state.handleToken({
    messageId: 'answer',
    delta: 'Successful answer',
    reasoning: null,
  })
  assert.equal(state.messagesByThread.value.a[1].content, 'Successful answer')
  assert.deepEqual(state.messagesByThread.value.b, [])
})

test('turn completion restores persisted step order and removes cancelled approvals', async () => {
  const persisted = [
    { id: 'u', role: 'user', content: 'Inspect' },
    { id: 'interim', role: 'assistant', content: 'Inspecting' },
    { id: 'call', role: 'tool_call', content: '' },
    { id: 'result', role: 'tool_result', content: 'Found' },
    {
      id: 'answer',
      role: 'assistant',
      content: 'Done',
      finishReason: 'cancelled',
    },
  ]
  const state = createChatState({ listMessagesAsync: async () => persisted })
  state.input.value = 'Inspect'
  await state.send()
  state.handleToken({
    messageId: 'answer',
    delta: 'InspectingDone',
    reasoning: null,
  })
  state.handleToolPermissionRequest({
    threadId: 'a',
    requestId: 'approval',
    toolName: 'run_command',
  })
  await state.handleTurnComplete({
    threadId: 'a',
    assistantMessageId: 'answer',
    finishReason: 'cancelled',
  })
  assert.deepEqual(state.messagesByThread.value.a, persisted)
  assert.deepEqual(state.pendingPrompts.value, [])
  assert.equal(state.busy.value, false)
  assert.equal(state.streamingMessageId.value, null)
})

test('completion clears approvals queued on a background conversation', async () => {
  const state = createChatState()
  state.input.value = 'Inspect'
  await state.send()
  state.handleToolPermissionRequest({
    threadId: 'a',
    requestId: 'approval',
    toolName: 'run_command',
  })
  await state.selectThread('b')
  await state.handleTurnComplete({
    threadId: 'a',
    assistantMessageId: 'answer',
    finishReason: 'cancelled',
  })
  await state.selectThread('a')
  assert.deepEqual(state.pendingPrompts.value, [])
})

test('a slower thread selection cannot restore approvals after a newer selection', async () => {
  let resolveB!: (messages: unknown[]) => void
  const state = createChatState({
    listMessagesAsync: async (threadId: string) => {
      if (threadId === 'b')
        return new Promise((resolve) => {
          resolveB = resolve
        })
      return []
    },
  })
  state.activeThreadId.value = 'a'
  state.pendingPrompts.value = [
    {
      kind: 'approval',
      requestId: 'approval-a',
      toolName: 'run_command',
      toolInput: {},
      riskClass: 'change',
    },
  ]

  const selectingB = state.selectThread('b')
  await flush()
  await state.selectThread('a')
  assert.equal(state.activeThreadId.value, 'a')
  assert.deepEqual(
    state.pendingPrompts.value.map((approval) => approval.requestId),
    ['approval-a'],
  )

  resolveB([])
  await selectingB
  assert.equal(state.activeThreadId.value, 'a')
  assert.deepEqual(
    state.pendingPrompts.value.map((approval) => approval.requestId),
    ['approval-a'],
  )
})

test('a terminal event before the invoke response cannot leave the composer busy', async () => {
  let acceptSend!: (value?: unknown) => void
  const state = createChatState({
    sendMessageAsync: () =>
      new Promise((resolve) => {
        acceptSend = resolve
      }),
  })
  state.input.value = 'Inspect'
  const sending = state.send()
  await flush()
  await state.handleTurnComplete({
    threadId: 'a',
    assistantMessageId: null,
    finishReason: 'error',
  })
  acceptSend({
    threadId: 'a',
    userMessageId: 'u',
    assistantMessageId: 'answer',
  })
  await sending
  assert.equal(state.busy.value, false)
  assert.deepEqual(state.messagesByThread.value.a, [])
})

test('history refresh failure surfaces the error and still releases the composer', async () => {
  const state = createChatState({
    listMessagesAsync: async () => {
      throw new Error('storage unavailable')
    },
  })
  state.input.value = 'Inspect'
  await state.send()
  await state.handleTurnComplete({
    threadId: 'a',
    assistantMessageId: 'answer',
    finishReason: 'cancelled',
  })
  assert.equal(state.busy.value, false)
  assert.match(state.lastError.value, /storage unavailable/)
  assert.equal(state.messagesByThread.value.a[1].finishReason, 'cancelled')
})

test('replaying a lost invoke response recovers a completed turn without new events or duplicate rows', async () => {
  let attempts = 0
  const requests: { idempotencyKey?: string }[] = []
  const persisted = [
    { id: 'u', role: 'user', content: 'Hello' },
    {
      id: 'answer',
      role: 'assistant',
      content: 'Already answered',
      finishReason: 'complete',
    },
  ]
  const state = createChatState({
    sendMessageAsync: async (request) => {
      requests.push(request)
      if (attempts++ === 0) throw new Error('invoke response lost')
      return { threadId: 'a', userMessageId: 'u', assistantMessageId: 'answer' }
    },
    listMessagesAsync: async () => persisted,
  })
  state.input.value = 'Hello'
  await state.send()
  assert.equal(state.busy.value, false)
  await state.send(true)
  assert.equal(requests[0].idempotencyKey, requests[1].idempotencyKey)
  assert.deepEqual(state.messagesByThread.value.a, persisted)
  assert.equal(state.busy.value, false)
  assert.equal(state.streamingMessageId.value, null)
})

test('leaving a chat with an approval pending aborts its active turn', async () => {
  let aborted = 0
  const state = createChatState({
    abortAsync: async () => {
      aborted++
    },
  })
  state.input.value = 'Inspect'
  await state.send()
  state.handleToolPermissionRequest({
    threadId: 'a',
    requestId: 'approval',
    toolName: 'run_command',
  })
  await state.unmount()
  assert.equal(aborted, 1)
})

const CHOICE = {
  requestId: 'choice',
  toolName: 'wm_app_open',
  question: null,
  field: 'appId',
  value: 'haex',
  options: [{ value: 'extension.mail', label: 'haex-mail' }],
}

test('an agent question queues after an approval of the active conversation, once', async () => {
  const state = createChatState()
  state.input.value = 'Open haex'
  await state.send()
  state.handleToolPermissionRequest({
    threadId: 'a',
    requestId: 'approval',
    toolName: 'run_command',
  })
  state.handleChoiceRequest({ threadId: 'a', ...CHOICE })
  state.handleChoiceRequest({ threadId: 'a', ...CHOICE })
  assert.deepEqual(
    state.pendingPrompts.value.map((prompt) => [prompt.kind, prompt.requestId]),
    [
      ['approval', 'approval'],
      ['choice', 'choice'],
    ],
  )
  assert.deepEqual(state.pendingPrompts.value[1], { kind: 'choice', ...CHOICE })
})

test('a question of a background conversation waits until it is opened', async () => {
  const state = createChatState()
  state.input.value = 'Open haex'
  await state.send()
  await state.selectThread('b')
  state.handleChoiceRequest({ threadId: 'a', ...CHOICE })
  assert.deepEqual(state.pendingPrompts.value, [])
  await state.selectThread('a')
  assert.deepEqual(
    state.pendingPrompts.value.map((prompt) => prompt.requestId),
    ['choice'],
  )
})

test('the end of the turn clears its open question', async () => {
  const state = createChatState()
  state.input.value = 'Open haex'
  await state.send()
  state.handleChoiceRequest({ threadId: 'a', ...CHOICE })
  await state.handleTurnComplete({
    threadId: 'a',
    assistantMessageId: 'answer',
    finishReason: 'cancelled',
  })
  assert.deepEqual(state.pendingPrompts.value, [])
})

test('leaving a chat with a question open aborts its active turn', async () => {
  let aborted = 0
  const state = createChatState({
    abortAsync: async () => {
      aborted++
    },
  })
  state.input.value = 'Open haex'
  await state.send()
  state.handleChoiceRequest({ threadId: 'a', ...CHOICE })
  await state.unmount()
  assert.equal(aborted, 1)
})

test('leaving while the initial request starts aborts before response IDs exist', async () => {
  let aborted = 0
  let rejectSend
  const state = createChatState({
    abortAsync: async () => {
      aborted++
      rejectSend(new Error('cancelled'))
    },
    sendMessageAsync: () =>
      new Promise((_, reject) => {
        rejectSend = reject
      }),
  })
  state.input.value = 'Inspect'
  const sending = state.send()
  await flush()
  await state.unmount()
  await sending
  assert.equal(aborted, 1)
  assert.equal(state.busy.value, false)
})

test('leaving while the session model loads does not send afterward', async () => {
  let resolveModel!: () => void
  let sends = 0
  const state = createChatState({
    useSessionModelAsync: () =>
      new Promise<void>((resolve) => {
        resolveModel = resolve
      }),
    sendMessageAsync: async () => {
      sends++
      return { threadId: 'a', userMessageId: 'u', assistantMessageId: 'answer' }
    },
  })
  state.input.value = 'Inspect'
  const sending = state.send()
  await flush()
  await state.unmount()
  resolveModel()
  await sending

  assert.equal(sends, 0)
  assert.equal(state.busy.value, false)
})

test('listener registrations finishing after unmount are disposed without continuing initialization', async () => {
  let finishRegistration!: (value?: unknown) => void
  let deviceReads = 0
  const released: string[] = []
  const subscriptions = [
    'onToken',
    'onMessageComplete',
    'onMessageError',
    'onToolCall',
    'onToolResult',
    'onRetry',
    'onTurnComplete',
    'onToolPermissionRequest',
    'onModelLoadProgress',
    'onModelLoadStatus',
  ]
  const state = createChatState(
    {
      ...Object.fromEntries(
        subscriptions.map((name) => [
          name,
          async () => () => released.push(name),
        ]),
      ),
      onToken: () =>
        new Promise((resolve) => {
          finishRegistration = resolve
        }),
    },
    {},
    {
      useDevice: () => ({
        currentDeviceInfoAsync: async () => {
          deviceReads++
          return {}
        },
      }),
      useModels: () => ({
        onDownloadProgress: async () => () => released.push('download'),
        onToolUseUpdated: async () => () => released.push('toolUseUpdated'),
      }),
    },
  )
  const mounting = state.mount()
  await nextTick()
  await state.unmount()
  finishRegistration(() => released.push('onToken'))
  await mounting
  assert.deepEqual(
    released.sort(),
    [...subscriptions, 'download', 'toolUseUpdated'].sort(),
  )
  assert.equal(deviceReads, 0)
})

test('a failed permission save restores the persisted mode and prevents overlapping changes', async () => {
  let rejectSave!: (reason?: unknown) => void
  let writes = 0
  const state = createChatState(
    {},
    {
      setPrefAsync: () => {
        writes++
        return new Promise((_, reject) => {
          rejectSave = reject
        })
      },
    },
  )
  await state.mount()
  const saving = state.updatePermissionMode('plan')
  assert.equal(state.permissionModeSaving.value, true)
  await state.updatePermissionMode('auto')
  assert.equal(writes, 1)
  rejectSave(new Error('preference write failed'))
  await saving
  assert.equal(state.permissionMode.value, 'manual')
  assert.equal(state.permissionModeSaving.value, false)
  assert.match(state.lastError.value, /preference write failed/)
})

// --- Spec 012: model capabilities drive the composer's effort control -------

/** A determined capability record offering the given reasoning option ids. */
const presetsCapabilities = (...ids: string[]) => ({
  reasoning: {
    kind: 'presets',
    options: ids.map((id) => ({ id, label: id })),
  },
  acceptedAttachmentKinds: ['text', 'image'],
  thinkingStyle: 'adaptive',
})
const localModel = (id: string, capabilities: unknown) => ({
  id,
  name: id,
  providerId: 'local',
  contextWindow: null,
  capabilities,
})
const providerModel = (
  providerId: string,
  id: string,
  capabilities: unknown,
) => ({
  id,
  name: id,
  providerId,
  contextWindow: null,
  capabilities,
})
const showModel = (
  state: ReturnType<typeof createChatState>,
  modelId: string,
) => {
  state.activeModel.value = {
    modelId,
    name: modelId,
    tokenizerRepo: '',
    contextWindow: null,
  }
}

test('the effort options offered follow the displayed model', async () => {
  const state = createChatState()
  state.modelStore.installedModels = [
    localModel('local-a', presetsCapabilities('low', 'high')),
  ]
  state.modelStore.providerModels = {
    p1: [
      providerModel(
        'p1',
        'p1:opus',
        presetsCapabilities('low', 'medium', 'high', 'xhigh', 'max'),
      ),
    ],
  }

  showModel(state, 'p1:opus')
  await nextTick()
  assert.equal(state.modelStore.effortState, 'selectable')
  assert.deepEqual(
    state.modelStore.effortOptions.map((o: { id: string }) => o.id),
    ['low', 'medium', 'high', 'xhigh', 'max'],
  )

  showModel(state, 'local-a')
  await nextTick()
  assert.deepEqual(
    state.modelStore.effortOptions.map((o: { id: string }) => o.id),
    ['low', 'high'],
  )
})

test('the effort control is hidden, managed or unknown by what the model says, and hidden for an unresolved model', async () => {
  const state = createChatState()
  state.modelStore.installedModels = [
    localModel('no-reasoning', {
      reasoning: { kind: 'unavailable' },
      acceptedAttachmentKinds: [],
      thinkingStyle: null,
    }),
    localModel('reasons-alone', {
      reasoning: { kind: 'model_managed' },
      acceptedAttachmentKinds: [],
      thinkingStyle: null,
    }),
    localModel('not-determined', null),
    localModel('empty-presets', presetsCapabilities()),
  ]

  const stateFor = async (modelId: string) => {
    showModel(state, modelId)
    await nextTick()
    return state.modelStore.effortState
  }

  assert.equal(await stateFor('no-reasoning'), 'hidden')
  assert.equal(await stateFor('reasons-alone'), 'managed')
  assert.equal(await stateFor('not-determined'), 'unknown')
  assert.equal(await stateFor('empty-presets'), 'hidden')
  // No row for this id (lists still loading, or the disabled "not
  // connected" placeholder): never described as "not yet known".
  assert.equal(await stateFor('delegate-claude:not-connected'), 'hidden')
})

test('a selection is accepted only from the offered options and is sent as the reasoning option', async () => {
  const requests: Array<{ reasoningOption?: string | null }> = []
  const state = createChatState({
    sendMessageAsync: async (request: { reasoningOption?: string | null }) => {
      requests.push(request)
      return { threadId: 'a', userMessageId: 'u', assistantMessageId: 'answer' }
    },
  })
  state.modelStore.installedModels = [
    localModel('model', presetsCapabilities('low', 'high')),
  ]
  await nextTick()

  state.modelStore.updateEffortLevel('xhigh')
  assert.equal(state.modelStore.effortLevel, null, 'not offered: rejected')
  state.modelStore.updateEffortLevel('high')
  assert.equal(state.modelStore.effortLevel, 'high')

  state.input.value = 'Hello'
  await state.send()
  assert.equal(requests[0].reasoningOption, 'high')

  state.modelStore.updateEffortLevel(null)
  assert.equal(state.modelStore.effortLevel, null)
})

test('switching models starts on Auto instead of carrying the previous choice over', async () => {
  const state = createChatState()
  state.modelStore.installedModels = [
    localModel('a', presetsCapabilities('low', 'high')),
    localModel('b', presetsCapabilities('low', 'high')),
  ]
  showModel(state, 'a')
  await nextTick()
  state.modelStore.updateEffortLevel('high')
  assert.equal(state.modelStore.effortLevel, 'high')

  showModel(state, 'b')
  await nextTick()

  assert.equal(state.modelStore.effortLevel, null)
})

test('a capability refresh that removes the selected option falls back to Auto', async () => {
  const state = createChatState()
  state.modelStore.installedModels = [
    localModel('model', presetsCapabilities('low', 'high', 'max')),
  ]
  await nextTick()
  state.modelStore.updateEffortLevel('max')
  assert.equal(state.modelStore.effortLevel, 'max')

  state.modelStore.installedModels = [
    localModel('model', presetsCapabilities('low', 'high')),
  ]
  await nextTick()

  assert.equal(state.modelStore.effortLevel, null)
})

test('switching the displayed model triggers no capability lookup', async () => {
  const invokeLog: string[] = []
  const state = createChatState({}, {}, {}, invokeLog)
  state.modelStore.installedModels = [
    localModel('a', presetsCapabilities('low')),
    localModel('b', presetsCapabilities('high')),
  ]
  await nextTick()
  invokeLog.length = 0

  showModel(state, 'a')
  await nextTick()
  showModel(state, 'b')
  await nextTick()

  assert.deepEqual(invokeLog, [])
})

// --- Spec 012: the effort choice is remembered per model and device --------

const EFFORT_KEY = (modelId: string) => `chat.reasoning_option.${modelId}`

/** An in-memory preference backend recording every call the store makes. */
function createPrefStore(initial: Record<string, string> = {}) {
  const map = new Map(Object.entries(initial))
  const calls: Array<{ op: string; scope: unknown; key: string }> = []
  const overrides = {
    getPrefAsync: async (scope: unknown, key: string) => {
      calls.push({ op: 'get', scope, key })
      return map.get(key) ?? null
    },
    setPrefAsync: async (scope: unknown, key: string, value: string) => {
      calls.push({ op: 'set', scope, key })
      map.set(key, value)
    },
    clearPrefAsync: async (scope: unknown, key: string) => {
      calls.push({ op: 'clear', scope, key })
      map.delete(key)
    },
  }
  return { map, calls, overrides }
}

/** Boots a store whose first displayed model is `first`, then initializes it. */
async function initializedStore(options: {
  prefs: ReturnType<typeof createPrefStore>
  installed: unknown[]
  first: string
}) {
  const state = createChatState(
    {
      activeLocalModelInfoAsync: async () => null,
      activeModelInfoAsync: async () => ({
        modelId: options.first,
        name: options.first,
        tokenizerRepo: '',
        contextWindow: null,
      }),
    },
    options.prefs.overrides,
    {
      useModels: () => ({
        listInstalledAsync: async () => options.installed,
        onDownloadProgress: async () => () => {},
      }),
    },
  )
  await state.modelStore.initialize()
  await flush()
  return state
}

test('a saved option is restored for the model shown first, read from the vault under the model key', async () => {
  const prefs = createPrefStore({ [EFFORT_KEY('a')]: 'high' })
  const state = await initializedStore({
    prefs,
    installed: [localModel('a', presetsCapabilities('low', 'high'))],
    first: 'a',
  })

  assert.equal(state.modelStore.effortLevel, 'high')
  assert.deepEqual(
    prefs.calls.find((c) => c.op === 'get' && c.key === EFFORT_KEY('a')),
    {
      op: 'get',
      scope: { kind: 'vault' },
      key: EFFORT_KEY('a'),
    },
  )
})

test('each model restores its own saved option and a fresh store restores it after a restart', async () => {
  const saved = {
    [EFFORT_KEY('a')]: 'high',
    [EFFORT_KEY('b')]: 'low',
  }
  const installed = [
    localModel('a', presetsCapabilities('low', 'high')),
    localModel('b', presetsCapabilities('low', 'high')),
  ]
  const state = await initializedStore({
    prefs: createPrefStore(saved),
    installed,
    first: 'a',
  })

  showModel(state, 'b')
  await flush()
  assert.equal(state.modelStore.effortLevel, 'low')
  showModel(state, 'a')
  await flush()
  assert.equal(state.modelStore.effortLevel, 'high')

  const restarted = await initializedStore({
    prefs: createPrefStore(saved),
    installed,
    first: 'b',
  })
  assert.equal(restarted.modelStore.effortLevel, 'low')
})

test('choosing an option stores it and choosing Auto clears it so nothing is stored', async () => {
  const prefs = createPrefStore()
  const state = await initializedStore({
    prefs,
    installed: [localModel('a', presetsCapabilities('low', 'high'))],
    first: 'a',
  })

  await state.modelStore.updateEffortLevel('high')
  assert.equal(prefs.map.get(EFFORT_KEY('a')), 'high')

  await state.modelStore.updateEffortLevel(null)
  assert.equal(state.modelStore.effortLevel, null)
  assert.equal(prefs.map.has(EFFORT_KEY('a')), false)
})

test('a saved option the model no longer offers falls back to Auto and clears the stale key', async () => {
  const prefs = createPrefStore({ [EFFORT_KEY('a')]: 'max' })
  const state = await initializedStore({
    prefs,
    installed: [localModel('a', presetsCapabilities('low', 'high'))],
    first: 'a',
  })

  assert.equal(state.modelStore.effortLevel, null)
  assert.equal(prefs.map.has(EFFORT_KEY('a')), false)
})

test('a capability refresh that removes the chosen option falls back to Auto and clears the stored choice', async () => {
  const prefs = createPrefStore()
  const state = await initializedStore({
    prefs,
    installed: [localModel('a', presetsCapabilities('low', 'high', 'max'))],
    first: 'a',
  })
  await state.modelStore.updateEffortLevel('max')
  assert.equal(prefs.map.get(EFFORT_KEY('a')), 'max')

  state.modelStore.installedModels = [
    localModel('a', presetsCapabilities('low', 'high')),
  ]
  await flush()

  assert.equal(state.modelStore.effortLevel, null)
  assert.equal(prefs.map.has(EFFORT_KEY('a')), false)
})

test('a saved option is kept, not cleared, while the model capabilities are still loading', async () => {
  const prefs = createPrefStore({ [EFFORT_KEY('a')]: 'high' })
  // The lists arrive empty: the row for 'a' is unresolved when the saved
  // option is read, so it cannot be validated yet.
  const state = await initializedStore({ prefs, installed: [], first: 'a' })
  assert.equal(state.modelStore.effortLevel, null)
  assert.equal(prefs.map.get(EFFORT_KEY('a')), 'high', 'must not be cleared')

  state.modelStore.installedModels = [
    localModel('a', presetsCapabilities('low', 'high')),
  ]
  await flush()

  assert.equal(state.modelStore.effortLevel, 'high')
})

test('a slow preference read for the previous model cannot overwrite the newly selected model', async () => {
  const prefs = createPrefStore({
    [EFFORT_KEY('a')]: 'high',
    [EFFORT_KEY('b')]: 'low',
  })
  let releaseA: () => void = () => {}
  const realGet = prefs.overrides.getPrefAsync
  prefs.overrides.getPrefAsync = async (scope: unknown, key: string) => {
    if (key === EFFORT_KEY('a')) {
      await new Promise<void>((resolve) => {
        releaseA = resolve
      })
    }
    return realGet(scope, key)
  }
  const state = await initializedStore({
    prefs,
    installed: [
      localModel('a', presetsCapabilities('low', 'high')),
      localModel('b', presetsCapabilities('low', 'high')),
    ],
    first: 'a',
  })

  showModel(state, 'b')
  await flush()
  assert.equal(state.modelStore.effortLevel, 'low')
  releaseA()
  await flush()

  assert.equal(state.modelStore.effortLevel, 'low')
})

test('a failed save rolls back to the previous choice and reports the failure', async () => {
  const prefs = createPrefStore()
  prefs.overrides.setPrefAsync = async () => {
    throw new Error('disk full')
  }
  const state = await initializedStore({
    prefs,
    installed: [localModel('a', presetsCapabilities('low', 'high'))],
    first: 'a',
  })

  await state.modelStore.updateEffortLevel('high')

  assert.equal(state.modelStore.effortLevel, null)
  assert.match(String(state.modelStore.lastError), /disk full/)
})

test('the same remote model reached through two connections stores its choice separately', async () => {
  const prefs = createPrefStore()
  const state = await initializedStore({
    prefs,
    installed: [],
    first: 'p1:opus',
  })
  state.modelStore.providerModels = {
    p1: [providerModel('p1', 'p1:opus', presetsCapabilities('low', 'high'))],
    p2: [providerModel('p2', 'p2:opus', presetsCapabilities('low', 'high'))],
  }
  await flush()

  await state.modelStore.updateEffortLevel('high')
  showModel(state, 'p2:opus')
  await flush()
  await state.modelStore.updateEffortLevel('low')

  showModel(state, 'p1:opus')
  await flush()
  assert.equal(state.modelStore.effortLevel, 'high')
  showModel(state, 'p2:opus')
  await flush()
  assert.equal(state.modelStore.effortLevel, 'low')

  assert.equal(prefs.map.get(EFFORT_KEY('p1:opus')), 'high')
  assert.equal(prefs.map.get(EFFORT_KEY('p2:opus')), 'low')
})

test('preference mutations for one model are persisted in call order', async () => {
  const prefs = createPrefStore()
  let releaseFirst!: () => void
  let writes = 0
  const realSet = prefs.overrides.setPrefAsync
  prefs.overrides.setPrefAsync = async (scope, key, value) => {
    writes++
    if (writes === 1) {
      await new Promise<void>((resolve) => {
        releaseFirst = resolve
      })
    }
    await realSet(scope, key, value)
  }
  const state = await initializedStore({
    prefs,
    installed: [localModel('a', presetsCapabilities('low', 'high'))],
    first: 'a',
  })

  const first = state.modelStore.updateEffortLevel('low')
  await flush()
  const second = state.modelStore.updateEffortLevel('high')
  await flush()

  assert.equal(writes, 1, 'the second write must wait for the first')
  releaseFirst()
  await Promise.all([first, second])
  assert.equal(prefs.map.get(EFFORT_KEY('a')), 'high')
})

test('capability reconciliation queues its clear behind a pending model write', async () => {
  const prefs = createPrefStore()
  let releaseWrite!: () => void
  const realSet = prefs.overrides.setPrefAsync
  prefs.overrides.setPrefAsync = async (scope, key, value) => {
    await new Promise<void>((resolve) => {
      releaseWrite = resolve
    })
    await realSet(scope, key, value)
  }
  const state = await initializedStore({
    prefs,
    installed: [localModel('a', presetsCapabilities('low', 'high', 'max'))],
    first: 'a',
  })

  const saving = state.modelStore.updateEffortLevel('max')
  await flush()
  state.modelStore.installedModels = [
    localModel('a', presetsCapabilities('low', 'high')),
  ]
  await flush()
  assert.equal(prefs.map.has(EFFORT_KEY('a')), false)

  releaseWrite()
  await saving
  await flush()
  assert.equal(prefs.map.has(EFFORT_KEY('a')), false)
})

test('no preference is read or written before the device is known, and the choice still works in memory', async () => {
  const prefs = createPrefStore({ [EFFORT_KEY('model')]: 'high' })
  const state = createChatState({}, prefs.overrides)
  state.modelStore.installedModels = [
    localModel('model', presetsCapabilities('low', 'high')),
  ]
  await flush()

  await state.modelStore.updateEffortLevel('low')

  assert.equal(state.modelStore.effortLevel, 'low')
  assert.deepEqual(prefs.calls, [])
})

// --- Spec 012: not yet known is never presented as unsupported -------------

test('a model with undetermined capabilities is reported as unknown in every shape, never as unavailable', async () => {
  const state = createChatState()
  state.modelStore.installedModels = [
    localModel('no-capabilities', null),
    // Attachments determined, reasoning not: the reasoning control is still
    // not known, and must not be inferred from the attachment answer.
    localModel('partial', {
      reasoning: null,
      acceptedAttachmentKinds: ['text'],
      thinkingStyle: null,
    }),
    localModel('nothing-determined', {
      reasoning: null,
      acceptedAttachmentKinds: null,
      thinkingStyle: null,
    }),
  ]

  for (const modelId of ['no-capabilities', 'partial', 'nothing-determined']) {
    showModel(state, modelId)
    await nextTick()
    assert.equal(state.modelStore.effortState, 'unknown', modelId)
    assert.deepEqual(state.modelStore.effortOptions, [], modelId)
  }
})

test('the backend reason for an undetermined attachment reaches the composer unchanged', async () => {
  const reason = 'attachment support for this model is not yet known'
  const state = createChatState({
    inspectAttachmentAsync: async () => ({
      name: 'photo.png',
      kind: 'image',
      usable: false,
      reason,
    }),
  })

  await state.addAttachments(['/tmp/photo.png'])

  assert.equal(state.attachments.value.length, 1)
  assert.equal(state.attachments.value[0].info.usable, false)
  assert.equal(state.attachments.value[0].info.reason, reason)
})

test('opening a session asks for that session’s model (spec 006)', async () => {
  const asked: (string | null)[] = []
  const state = createChatState({
    useSessionModelAsync: async (threadId: string | null) => {
      asked.push(threadId)
    },
  })
  await state.selectThread('b')
  await state.selectThread('a')
  assert.deepEqual(asked, ['b', 'a'])
})

test('only a pick in the model menu is remembered for new sessions (spec 002 FR-009, 2026-11-02)', async () => {
  const picked: string[] = []
  const loaded: string[] = []
  const info = (modelId: string) => ({
    modelId,
    name: modelId,
    tokenizerRepo: '',
    contextWindow: null,
  })
  const state = createChatState({
    selectModelAsync: async (modelId: string) => {
      picked.push(modelId)
      return info(modelId)
    },
    loadModelAsync: async (modelId: string) => {
      loaded.push(modelId)
      return info(modelId)
    },
  })
  await state.modelStore.loadModel('qwen3-4b', { pick: true })
  await state.modelStore.loadModel('retry-model')
  assert.deepEqual(picked, ['qwen3-4b'])
  assert.deepEqual(loaded, ['retry-model'])
  assert.equal(state.modelStore.activeModel?.modelId, 'retry-model')
})

// The active local model (AI settings rework): one local model is held in memory until it is
// unloaded in the settings, apart from the chat session's model, and it is the only local model
// the chat offers.
test('the chat offers only the active local model of the installed ones', () => {
  const state = createChatState()
  state.modelStore.installedModels = [
    { id: 'qwen3-4b', name: 'Qwen 3 4B' },
    { id: 'qwen3.5-4b', name: 'Qwen 3.5 4B' },
  ]

  assert.deepEqual(
    state.modelStore.modelGroups.filter(
      (g: { providerId: string }) => g.providerId === 'local',
    ),
    [],
    'nothing is active: no local model is offered',
  )

  state.modelStore.activeLocalModel = {
    modelId: 'qwen3.5-4b',
    name: 'Qwen 3.5 4B',
  }
  const local = state.modelStore.modelGroups.find(
    (g: { providerId: string }) => g.providerId === 'local',
  )
  assert.deepEqual(
    local?.models.map((m: { id: string }) => m.id),
    ['qwen3.5-4b'],
  )
})

test('activating a local model leaves the chat session on its own model', async () => {
  let selected = 0
  const state = createChatState({
    activateLocalModelAsync: async (modelId: string) => ({
      modelId,
      name: 'Qwen 3.5 4B',
      tokenizerRepo: '',
      contextWindow: null,
    }),
    selectModelAsync: async () => {
      selected++
      throw new Error('the chat must not pick a model')
    },
  })
  state.modelStore.installedModels = [{ id: 'qwen3.5-4b', name: 'Qwen 3.5 4B' }]

  await state.modelStore.activateLocalModel('qwen3.5-4b')

  assert.equal(state.modelStore.activeLocalModelId, 'qwen3.5-4b')
  assert.equal(
    state.modelStore.activeModel?.modelId,
    'model',
    'the chat session keeps the model it had',
  )
  assert.equal(selected, 0)
  assert.equal(state.modelStore.lastError, null)
})

test('a failed activation keeps the model that was active and says why', async () => {
  const state = createChatState({
    activateLocalModelAsync: async () => {
      throw new Error('not enough memory')
    },
    // The backend replaces the active model only when the activation succeeds.
    activeLocalModelInfoAsync: async () => ({
      modelId: 'qwen3-4b',
      name: 'Qwen 3 4B',
      tokenizerRepo: '',
      contextWindow: null,
    }),
  })
  state.modelStore.installedModels = [
    { id: 'qwen3-4b', name: 'Qwen 3 4B' },
    { id: 'qwen3.5-4b', name: 'Qwen 3.5 4B' },
  ]
  await state.modelStore.refreshActiveLocalModel()

  await state.modelStore.activateLocalModel('qwen3.5-4b')

  assert.equal(state.modelStore.activeLocalModelId, 'qwen3-4b')
  assert.match(state.modelStore.lastError, /not enough memory/)
  assert.equal(state.modelStore.modelLoadPending, false)
})

test('unloading clears the active local model and re-reads the chat session', async () => {
  let sessionReads = 0
  const state = createChatState({
    deactivateLocalModelAsync: async () => {},
    activeModelInfoAsync: async () => {
      sessionReads++
      return null
    },
  })
  state.modelStore.activeLocalModel = {
    modelId: 'qwen3-4b',
    name: 'Qwen 3 4B',
  }

  await state.modelStore.deactivateLocalModel()

  assert.equal(state.modelStore.activeLocalModelId, null)
  assert.equal(sessionReads, 1, 'a session on that model ended with it')
  assert.equal(state.modelStore.activeModel, null)
})
