// Part of `pnpm check:chat-state` (spec 020-tab-navigation, T057): the chat page's tab history —
// conversations as entries (`/`, `/thread/<id>`), replace on thread creation, skipping entries of
// deleted conversations (FR-027, research R10). Runs the real `ChatApp.vue` script block on the
// shared replay harness with its recording tab router. A separate file because
// `check-chat-state.ts` is already far past the 500-line limit.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  createChatState,
  createRecordingTabRouter,
  flush,
} from './lib/chat-state-harness.ts'

const THREADS = [
  { id: 'a', title: 'A', lastModelId: null, createdAt: 1, updatedAt: 1 },
  { id: 'b', title: 'B', lastModelId: null, createdAt: 2, updatedAt: 2 },
]

function chatWithRouter(overrides: Record<string, unknown> = {}) {
  const tabRouter = createRecordingTabRouter()
  const state = createChatState(
    { listMessagesAsync: async () => [], ...overrides },
    {},
    {},
    undefined,
    { tabRouter },
  )
  state.threads.value = THREADS.map((thread) => ({ ...thread }))
  void state.syncFromLocation()
  return { state, tabRouter }
}

test('reaching a conversation location opens it; back and forward move between entries', async () => {
  const { state, tabRouter } = chatWithRouter()
  tabRouter.push('/thread/a')
  await flush()
  assert.equal(state.activeThreadId.value, 'a')
  tabRouter.push('/thread/b')
  await flush()
  assert.equal(state.activeThreadId.value, 'b')
  tabRouter.back()
  await flush()
  assert.equal(state.activeThreadId.value, 'a')
  tabRouter.back()
  await flush()
  assert.equal(state.activeThreadId.value, null)
  tabRouter.forward()
  await flush()
  assert.equal(state.activeThreadId.value, 'a')
})

test('the first message of a new conversation replaces `/` with its thread location', async () => {
  const { state, tabRouter } = chatWithRouter({
    sendMessageAsync: async () => ({
      threadId: 'new',
      userMessageId: 'u',
      assistantMessageId: 'answer',
    }),
  })
  state.input.value = 'Hello'
  await state.send()
  await flush()
  assert.equal(state.activeThreadId.value, 'new')
  assert.equal(tabRouter.route.path, '/thread/new')
  assert.deepEqual(tabRouter.entries, ['/thread/new'])
  assert.ok(tabRouter.log.includes('replace /thread/new'))
})

test('an entry of a deleted conversation is skipped instead of showing it', async () => {
  const { state, tabRouter } = chatWithRouter()
  tabRouter.push('/thread/a')
  await flush()
  tabRouter.push('/thread/b')
  await flush()
  state.threads.value = state.threads.value.filter((t) => t.id !== 'a')
  tabRouter.back()
  await flush()
  assert.ok(tabRouter.log.includes('skip'))
  assert.equal(tabRouter.route.path, '/')
  assert.equal(state.activeThreadId.value, null)
  assert.ok(!tabRouter.entries.includes('/thread/a'))
})

test('a conversation id that never existed falls back to the start location', async () => {
  const { state, tabRouter } = chatWithRouter()
  tabRouter.replace('/thread/ghost')
  await flush()
  assert.equal(tabRouter.route.path, '/')
  assert.equal(state.activeThreadId.value, null)
})

test('an initial conversation location is applied after threads load', async () => {
  const tabRouter = createRecordingTabRouter()
  tabRouter.replace('/thread/a')
  const state = createChatState({}, {}, {}, undefined, { tabRouter })
  await state.mount()
  assert.equal(state.activeThreadId.value, 'a')
  assert.equal(tabRouter.route.path, '/thread/a')
})

// Spec 072: a contact or group conversation is a location of its own (`/contact/<id>`). It opens on
// top of the agent conversation, which keeps its state underneath.
test('a contact conversation location opens it and leaves the agent conversation as it is', async () => {
  const { state, tabRouter } = chatWithRouter()
  tabRouter.push('/thread/a')
  await flush()
  tabRouter.push('/contact/ben')
  await flush()
  assert.equal(state.activePeerId.value, 'ben')
  assert.equal(state.activeThreadId.value, 'a', 'the agent conversation stays')
  assert.equal(tabRouter.route.path, '/contact/ben', 'nothing aligns it away')
  tabRouter.back()
  await flush()
  assert.equal(state.activePeerId.value, null)
  assert.equal(state.activeThreadId.value, 'a')
  tabRouter.forward()
  await flush()
  assert.equal(state.activePeerId.value, 'ben')
})

test('an initial contact conversation location is applied on mount', async () => {
  const tabRouter = createRecordingTabRouter()
  tabRouter.replace('/contact/ben')
  const state = createChatState({}, {}, {}, undefined, { tabRouter })
  await state.mount()
  assert.equal(state.activePeerId.value, 'ben')
  assert.equal(tabRouter.route.path, '/contact/ben')
})

test('starting a new agent conversation from a contact conversation leaves it', async () => {
  const { state, tabRouter } = chatWithRouter()
  tabRouter.push('/contact/ben')
  await flush()
  tabRouter.push('/')
  await flush()
  assert.equal(state.activePeerId.value, null)
  assert.equal(state.activeThreadId.value, null)
})
