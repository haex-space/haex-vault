// `pnpm check:chat-state` (spec 032 US4, FR-016/017/023): the notice about a model's tool use is
// shown once per conversation and state, and never when the model got its tools without doubt.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  canSetRights,
  createToolNotices,
  delegateNoticeKey,
  noticeKey,
  restrictedKey,
  type ProfileMissing,
  type ToolAvailability,
} from '../src/lib/chat/toolNotice.ts'
import type { DelegateStatus } from '../src/types/bindings/DelegateStatus.ts'

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
    'chat.toolNotice.delegate',
    'chat.toolNotice.unsupported',
    'chat.toolNotice.unverified',
  ])
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
  assert.equal(notices.show('a', 'delegate'), true)
})

test('an offered turn in between does not use up a later notice', () => {
  const notices = createToolNotices()
  assert.equal(notices.show('a', 'offered'), false)
  assert.equal(notices.show('a', 'offeredUnverified'), true)
})

const DELEGATE_STATUSES: DelegateStatus[] = [
  'off',
  'unregistered',
  'expired',
  'registered',
]

test('a registered delegate needs no notice (spec 021 US6)', () => {
  assert.equal(delegateNoticeKey('registered'), null)
  assert.equal(canSetRights('registered'), false)
})

test('every other delegate status has its own text', () => {
  const keys = DELEGATE_STATUSES.map(delegateNoticeKey).filter(
    (key) => key !== null,
  )
  assert.deepEqual(keys.sort(), [
    'chat.toolNotice.delegateExpired',
    'chat.toolNotice.delegateOff',
    'chat.toolNotice.delegateUnregistered',
  ])
})

test('"Rechte festlegen" is offered without rights, never with the access off', () => {
  assert.deepEqual(DELEGATE_STATUSES.filter(canSetRights), [
    'unregistered',
    'expired',
  ])
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

test('the notice about an agent profile shows once per missing right and conversation', () => {
  const notices = createToolNotices()
  assert.equal(notices.showRestricted('a', 'p1', { kind: 'browser' }), true)
  assert.equal(notices.showRestricted('a', 'p1', { kind: 'browser' }), false)
  assert.equal(notices.showRestricted('a', 'p1', { kind: 'passwords' }), true)
  assert.equal(notices.showRestricted('a', 'p2', { kind: 'browser' }), true)
  assert.equal(notices.showRestricted('b', 'p1', { kind: 'browser' }), true)
})
