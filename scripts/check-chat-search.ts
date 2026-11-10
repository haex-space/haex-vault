// Part of `pnpm check:chat-state` (spec 076 US2, FR-011, Edge Cases): the chat sidebar's message
// search — the minimum length, one search per burst of keys after the debounce, a late result of an
// older text never replacing a newer one, clearing and errors. The command is replaced, the clock
// is node's mock timers.
import assert from 'node:assert/strict'
import { mock, test } from 'node:test'
import { nextTick } from 'vue'

import {
  SEARCH_DEBOUNCE_MS,
  useChatSearch,
  type SearchResult,
} from '../src/composables/useChatSearch.ts'

const errors: unknown[] = []
const onError = (e: unknown) => errors.push(e)

function hit(messageId: string) {
  return {
    threadId: 't',
    kind: 'direct' as const,
    messageId,
    at: 1,
    snippet: messageId,
  }
}

function result(...ids: string[]): SearchResult {
  return { hits: ids.map(hit), more: false }
}

/** Lets the watcher see the change, the debounce run out and the command answer. */
async function settle() {
  await nextTick()
  mock.timers.tick(SEARCH_DEBOUNCE_MS)
  for (let i = 0; i < 5; i++) await Promise.resolve()
}

test.beforeEach(() => mock.timers.enable({ apis: ['setTimeout'] }))
test.afterEach(() => mock.timers.reset())

test('one character does not search; two do', async () => {
  const asked: string[] = []
  const search = useChatSearch(onError, async (text) => {
    asked.push(text)
    return result('m1')
  })

  search.text.value = 'a'
  await settle()
  assert.deepEqual(asked, [])
  assert.equal(search.hits.value.length, 0)

  search.text.value = ' ä '
  await settle()
  assert.deepEqual(asked, [])

  search.text.value = 'ab'
  await settle()
  assert.deepEqual(asked, ['ab'])
  assert.deepEqual(
    search.hits.value.map((h) => h.messageId),
    ['m1'],
  )
})

test('a burst of keys searches once, after the debounce', async () => {
  const asked: string[] = []
  const search = useChatSearch(onError, async (text) => {
    asked.push(text)
    return result()
  })

  for (const text of ['ba', 'bah', 'bahn']) {
    search.text.value = text
    await nextTick()
    mock.timers.tick(SEARCH_DEBOUNCE_MS - 50)
  }
  assert.deepEqual(asked, [])
  await settle()
  assert.deepEqual(asked, ['bahn'])
})

test('a late result of an older text never replaces a newer one', async () => {
  const pending = new Map<string, (value: SearchResult) => void>()
  const search = useChatSearch(
    onError,
    (text) => new Promise((resolve) => pending.set(text, resolve)),
  )

  search.text.value = 'alt'
  await settle()
  search.text.value = 'neu'
  await settle()

  pending.get('neu')!(result('new'))
  await settle()
  pending.get('alt')!(result('old'))
  await settle()

  assert.deepEqual(
    search.hits.value.map((h) => h.messageId),
    ['new'],
  )
  assert.equal(search.searching.value, false)
})

test('a result for a text the user already changed is not shown', async () => {
  let answer: ((value: SearchResult) => void) | undefined
  const search = useChatSearch(
    onError,
    () => new Promise((resolve) => (answer = resolve)),
  )

  search.text.value = 'alt'
  await settle()
  search.text.value = 'altx'
  await nextTick()
  answer!(result('old'))
  for (let i = 0; i < 5; i++) await Promise.resolve()

  assert.equal(search.hits.value.length, 0)
})

test('clearing empties the hits and drops a running search', async () => {
  let answer: ((value: SearchResult) => void) | undefined
  const search = useChatSearch(onError, async (text) =>
    text === 'eins'
      ? result('m1')
      : new Promise((resolve) => (answer = resolve)),
  )

  search.text.value = 'eins'
  await settle()
  assert.equal(search.hits.value.length, 1)

  search.text.value = 'zwei'
  await settle()
  search.clear()
  answer!(result('m2'))
  await settle()

  assert.equal(search.text.value, '')
  assert.equal(search.hits.value.length, 0)
  assert.equal(search.more.value, false)
})

test('an error is reported once and the hits stay empty', async () => {
  errors.length = 0
  const search = useChatSearch(onError, async () => {
    throw 'locked'
  })

  search.text.value = 'abc'
  await settle()

  assert.deepEqual(errors, ['locked'])
  assert.equal(search.failed.value, true)
  assert.equal(search.hits.value.length, 0)
  assert.equal(search.searching.value, false)
})

test('an error of a text the user already changed is not reported', async () => {
  errors.length = 0
  let fail: ((reason: unknown) => void) | undefined
  const search = useChatSearch(
    onError,
    () => new Promise((_, reject) => (fail = reject)),
  )

  search.text.value = 'alt'
  await settle()
  search.text.value = 'altx'
  await nextTick()
  fail!('locked')
  for (let i = 0; i < 5; i++) await Promise.resolve()

  assert.deepEqual(errors, [])
  assert.equal(search.failed.value, false)
})
