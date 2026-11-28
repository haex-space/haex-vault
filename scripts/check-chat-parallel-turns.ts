// Run with `node scripts/check-chat-parallel-turns.ts`. Spec 068 research R1 (T015): several
// conversations run turns side by side; the turn of a conversation the person left keeps running,
// parked, and comes back with its conversation. Replays the real page and composables through the
// harness in `scripts/lib/chat-state-harness.ts`, as `check-chat-state.ts` does.
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { createChatState, flush } from './lib/chat-state-harness.ts'

interface SendRequest {
  threadId?: string | null
  content: string
}

/** A chat whose sends go to the thread they name (a new conversation starts thread `a`). */
function chatWithTwoThreads(extra: Record<string, unknown> = {}) {
  const aborted: (string | null | undefined)[] = []
  const requests: SendRequest[] = []
  const overrides = {
    sendMessageAsync: async (request: SendRequest) => {
      requests.push(request)
      const thread = request.threadId === 'b' ? 'b' : 'a'
      return {
        threadId: thread,
        userMessageId: `user-${thread}`,
        assistantMessageId: `answer-${thread}`,
      }
    },
    abortAsync: async (threadId?: string | null) => {
      aborted.push(threadId)
    },
    ...extra,
  }
  return { state: createChatState(overrides), aborted, requests }
}

function assistantOf(
  state: ReturnType<typeof createChatState>,
  thread: 'a' | 'b',
) {
  return state.messagesByThread.value[thread]?.find(
    (message: { id: string }) => message.id === `answer-${thread}`,
  )
}

async function startTurnsInBothThreads(
  state: ReturnType<typeof createChatState>,
) {
  state.input.value = 'one'
  await state.send()
  await state.selectThread('b')
  state.input.value = 'two'
  await state.send()
}

test('a second conversation sends while the first one still runs', async () => {
  const { state } = chatWithTwoThreads()
  state.input.value = 'one'
  await state.send()
  assert.equal(state.busy.value, true)
  assert.equal(state.streamingThreadId.value, 'a')

  await state.selectThread('b')
  // The open conversation has no turn: its composer is free although `a` still runs.
  assert.equal(state.busy.value, false)
  assert.equal(state.streamingMessageId.value, null)
  assert.equal(state.sendDisabled.value, false)

  state.input.value = 'two'
  await state.send()
  assert.equal(state.busy.value, true)
  assert.equal(state.streamingThreadId.value, 'b')
  assert.equal(state.streamingMessageId.value, 'answer-b')
  assert.ok(state.streams.isParked('a'), 'the first turn runs parked')
})

test('tokens of both turns reach their own conversation', async () => {
  const { state } = chatWithTwoThreads()
  await startTurnsInBothThreads(state)

  state.handleToken({ messageId: 'answer-a', delta: 'From A', reasoning: null })
  state.handleToken({ messageId: 'answer-b', delta: 'From B', reasoning: null })
  state.handleToken({ messageId: 'answer-a', delta: '!', reasoning: 'thinks' })

  assert.equal(assistantOf(state, 'a')?.content, 'From A!')
  assert.equal(assistantOf(state, 'b')?.content, 'From B')
  assert.equal(state.reasoningByMessage.value['answer-a'], 'thinks')
})

test('a retry of the parked turn clears only its own text', async () => {
  const { state } = chatWithTwoThreads()
  await startTurnsInBothThreads(state)
  state.handleToken({ messageId: 'answer-a', delta: 'bad', reasoning: null })
  state.handleToken({ messageId: 'answer-b', delta: 'fine', reasoning: null })

  state.handleRetry({
    threadId: 'a',
    assistantMessageId: 'answer-a',
    attempt: 1,
  })

  assert.equal(assistantOf(state, 'a')?.content, '')
  assert.equal(assistantOf(state, 'b')?.content, 'fine')
})

test('the parked turn ending leaves the open turn running', async () => {
  const { state } = chatWithTwoThreads()
  await startTurnsInBothThreads(state)

  await state.handleTurnComplete({
    threadId: 'a',
    assistantMessageId: 'answer-a',
    finishReason: 'complete',
  })

  assert.equal(state.streams.isParked('a'), false)
  assert.equal(state.busy.value, true, 'the open conversation is still busy')
  assert.equal(state.streamingMessageId.value, 'answer-b')
  assert.equal(state.streamingThreadId.value, 'b')

  await state.handleTurnComplete({
    threadId: 'b',
    assistantMessageId: 'answer-b',
    finishReason: 'complete',
  })
  assert.equal(state.busy.value, false)
  assert.equal(state.streamingMessageId.value, null)
})

test('the open turn ending leaves the parked turn running', async () => {
  const { state } = chatWithTwoThreads()
  await startTurnsInBothThreads(state)

  await state.handleTurnComplete({
    threadId: 'b',
    assistantMessageId: 'answer-b',
    finishReason: 'complete',
  })
  assert.equal(state.busy.value, false)
  assert.ok(state.streams.isParked('a'), 'the parked turn is not touched')

  state.handleToken({ messageId: 'answer-a', delta: 'still', reasoning: null })
  assert.equal(assistantOf(state, 'a')?.content, 'still')
})

test('opening the conversation of a running turn brings its state back', async () => {
  const { state } = chatWithTwoThreads()
  state.input.value = 'one'
  await state.send()
  await state.selectThread('b')
  state.handleToken({ messageId: 'answer-a', delta: 'Hi', reasoning: null })

  await state.selectThread('a')

  assert.equal(state.busy.value, true)
  assert.equal(state.streamingMessageId.value, 'answer-a')
  assert.equal(state.streamingThreadId.value, 'a')
  assert.equal(state.streams.isParked('a'), false)
  assert.equal(state.sendDisabled.value, true, 'one turn per conversation')

  // The restored turn goes on as the open one and ends the normal way.
  state.handleToken({ messageId: 'answer-a', delta: '!', reasoning: null })
  assert.equal(assistantOf(state, 'a')?.content, 'Hi!')
  await state.handleTurnComplete({
    threadId: 'a',
    assistantMessageId: 'answer-a',
    finishReason: 'complete',
  })
  assert.equal(state.busy.value, false)
})

test('stopping stops the turn of the open conversation only', async () => {
  const { state, aborted } = chatWithTwoThreads()
  await startTurnsInBothThreads(state)

  await state.abort()
  assert.deepEqual(aborted, ['b'])

  await state.selectThread('a')
  await state.abort()
  assert.deepEqual(aborted, ['b', 'a'])
})

test('closing the tab stops every turn', async () => {
  const { state, aborted } = chatWithTwoThreads()
  await startTurnsInBothThreads(state)
  assert.equal(state.streams.anyRunning(), true)

  await state.abortAll()

  assert.deepEqual(
    aborted,
    [undefined],
    'one call without a conversation: all turns',
  )
})

test('a new conversation starts while a turn runs and the turn keeps streaming', async () => {
  const { state } = chatWithTwoThreads()
  state.input.value = 'one'
  await state.send()

  await state.newChat()

  assert.equal(state.activeThreadId.value, null)
  assert.equal(state.busy.value, false)
  assert.ok(state.streams.isParked('a'))
  state.handleToken({ messageId: 'answer-a', delta: 'Hi', reasoning: 'why' })
  assert.equal(assistantOf(state, 'a')?.content, 'Hi')
  assert.equal(
    state.reasoningByMessage.value['answer-a'],
    'why',
    'clearing the new conversation keeps what the parked turn streams',
  )
})

test('a parked turn that ends while another send is being set up is not held back', async () => {
  let acceptSend!: (value: unknown) => void
  const reloaded: string[] = []
  const { state } = chatWithTwoThreads({
    sendMessageAsync: (request: SendRequest) =>
      request.threadId === 'b'
        ? new Promise((resolve) => {
            acceptSend = resolve
          })
        : Promise.resolve({
            threadId: 'a',
            userMessageId: 'user-a',
            assistantMessageId: 'answer-a',
          }),
    listMessagesAsync: async (threadId: string) => {
      reloaded.push(threadId)
      return []
    },
  })
  state.input.value = 'one'
  await state.send()
  await state.selectThread('b')
  // The send of the second conversation is still on its way.
  state.input.value = 'two'
  const sending = state.send()
  await flush()

  await state.handleTurnComplete({
    threadId: 'a',
    assistantMessageId: 'answer-a',
    finishReason: 'complete',
  })

  assert.equal(state.streams.isParked('a'), false, 'the parked turn ended')
  assert.ok(reloaded.filter((id) => id === 'a').length >= 1)
  acceptSend({
    threadId: 'b',
    userMessageId: 'user-b',
    assistantMessageId: 'answer-b',
  })
  await sending
  assert.equal(state.streamingThreadId.value, 'b')
})

test('deleting a conversation stops its parked turn and forgets it', async () => {
  const deleted: string[] = []
  const { state, aborted } = chatWithTwoThreads({
    deleteThreadAsync: async (threadId: string) => {
      deleted.push(threadId)
    },
  })
  state.threads.value = [
    { id: 'a', title: 'A', lastModelId: null, createdAt: 1, updatedAt: 2 },
    { id: 'b', title: 'B', lastModelId: null, createdAt: 1, updatedAt: 2 },
  ]
  state.input.value = 'one'
  await state.send()
  await state.selectThread('b')

  state.requestDelete(state.threads.value[0])
  const deleting = state.confirmDelete()
  await flush()
  // The abort names the conversation being deleted, not the open one.
  assert.deepEqual(aborted, ['a'])
  await state.handleTurnComplete({
    threadId: 'a',
    assistantMessageId: 'answer-a',
    finishReason: 'cancelled',
  })
  await deleting

  assert.equal(state.streams.isParked('a'), false)
  assert.deepEqual(
    deleted,
    ['a'],
    'the conversation is deleted after its turn ended',
  )
})
