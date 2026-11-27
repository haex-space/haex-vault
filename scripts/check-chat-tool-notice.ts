// `pnpm check:chat-state` (spec 032 US4, FR-016/017/023): the notice about a model's tool use is
// shown once per conversation and state, and never when the model got its tools without doubt.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  createToolNotices,
  DELEGATE_OWN_WEB_KEY,
  DELEGATE_UNAVAILABLE_KEY,
  noticeKey,
  restrictedKey,
  type ProfileMissing,
  type ToolAvailability,
} from '../src/lib/chat/toolNotice.ts'

const STATES: ToolAvailability[] = [
  'offered',
  'offeredUnverified',
  'unsupported',
  'delegate',
]

test('a state that offers tools without doubt has no notice', () => {
  assert.equal(noticeKey('offered'), null)
  assert.equal(createToolNotices().show('a', 'offered'), false)
})

test('every other state has its own notice text', () => {
  const keys = STATES.map(noticeKey).filter((key) => key !== null)
  assert.deepEqual(keys.sort(), [
    'chat.toolNotice.unsupported',
    'chat.toolNotice.unverified',
  ])
  assert.equal(noticeKey('delegate'), null)
})

test('a notice shows once per conversation and state', () => {
  const notices = createToolNotices()
  assert.equal(notices.show('a', 'unsupported'), true)
  assert.equal(notices.show('a', 'unsupported'), false)
  assert.equal(notices.show('a', 'unsupported'), false)
})

test('another conversation or another state shows its own notice', () => {
  const notices = createToolNotices()
  assert.equal(notices.show('a', 'unsupported'), true)
  assert.equal(notices.show('b', 'unsupported'), true)
  assert.equal(notices.show('a', 'offeredUnverified'), true)
  assert.equal(notices.show('a', 'delegate'), false)
})

test('an offered turn in between does not use up a later notice', () => {
  const notices = createToolNotices()
  assert.equal(notices.show('a', 'offered'), false)
  assert.equal(notices.show('a', 'offeredUnverified'), true)
})

test('each delegate start without haex-vault tools gets its own notice (spec 102 FR-012)', async () => {
  const notices = createToolNotices()
  assert.equal(notices.showUnavailable(), true)
  assert.equal(notices.showUnavailable(), true)

  const { readFileSync } = await import('node:fs')
  for (const language of ['de', 'en']) {
    const texts = JSON.parse(
      readFileSync(
        new URL(`../src/i18n/locales/${language}.json`, import.meta.url),
        'utf8',
      ),
    ).chat.toolNotice
    const key = DELEGATE_UNAVAILABLE_KEY.split('.').at(-1) ?? ''
    assert.ok(texts[key]?.includes('{name}'), `${language}: ${key}`)
    assert.ok(texts[key]?.includes('{reason}'), `${language}: ${key}`)
    // The notice that sent the person to the external access is gone (spec 102 FR-012).
    assert.equal(texts.delegateOff, undefined, `${language}: delegateOff`)
  }
})

const MISSING: ProfileMissing[] = [
  { kind: 'browser' },
  { kind: 'passwords' },
  { kind: 'topic', topic: 'chat.write' },
  { kind: 'topicWrite', topic: 'chat.write' },
  { kind: 'commands' },
  { kind: 'extension', name: 'Mail' },
]

test('what an agent profile does not give has a text per kind, in both languages (spec 083 US5)', async () => {
  const { readFileSync } = await import('node:fs')
  for (const language of ['de', 'en']) {
    const texts = JSON.parse(
      readFileSync(
        new URL(`../src/i18n/locales/${language}.json`, import.meta.url),
        'utf8',
      ),
    ).chat.agentProfile.restricted
    for (const missing of MISSING) {
      assert.ok(texts[missing.kind], `${language}: ${missing.kind}`)
      assert.ok(texts[missing.kind].includes('{profile}'), missing.kind)
      assert.equal(
        restrictedKey(missing),
        `chat.agentProfile.restricted.${missing.kind}`,
      )
    }
    assert.ok(texts.open, `${language}: open`)
  }
})

test("the notice about a delegate's own web access shows once per conversation (spec 102 FR-013)", async () => {
  const notices = createToolNotices()
  assert.equal(notices.showOwnWeb('a'), true)
  assert.equal(notices.showOwnWeb('a'), false)
  assert.equal(notices.showOwnWeb('b'), true)
  // It does not use up another notice of the same conversation.
  assert.equal(notices.show('a', 'unsupported'), true)

  const { readFileSync } = await import('node:fs')
  for (const language of ['de', 'en']) {
    const texts = JSON.parse(
      readFileSync(
        new URL(`../src/i18n/locales/${language}.json`, import.meta.url),
        'utf8',
      ),
    ).chat.toolNotice
    const key = DELEGATE_OWN_WEB_KEY.split('.').at(-1) ?? ''
    assert.ok(texts[key]?.includes('{name}'), `${language}: ${key}`)
  }
})

test('the notice about an agent profile shows once per missing right and conversation', () => {
  const notices = createToolNotices()
  assert.equal(notices.showRestricted('a', 'p1', { kind: 'browser' }), true)
  assert.equal(notices.showRestricted('a', 'p1', { kind: 'browser' }), false)
  assert.equal(notices.showRestricted('a', 'p1', { kind: 'passwords' }), true)
  assert.equal(notices.showRestricted('a', 'p2', { kind: 'browser' }), true)
  assert.equal(notices.showRestricted('b', 'p1', { kind: 'browser' }), true)
})
