// `pnpm check:agent-profiles` (spec 083): when a change of a profile's rights gives its agents more
// than they had (`widens`, research R11), and the catalog rules of the profile actions: all five are
// guardrails no agent can call. Pure modules, loaded with Node type-stripping.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import { ALL_ACTIONS } from '../src/lib/actions/catalog.ts'
import {
  COMPARED_RIGHTS,
  UNCONFIRMED_RIGHTS,
  STANDARD_PROFILE_ID,
  kindLabelKey,
  profileErrorCode,
  profileLabel,
  widens,
} from '../src/lib/agents/profiles.ts'
import { topicKeyOf, type Rights } from '../src/lib/agents/rights.ts'

const NOTHING: Rights = { operate: {} }

/** Rights with `extra` on top of an empty set. */
const rights = (extra: Partial<Rights> = {}): Rights => ({
  operate: {},
  ...extra,
})

const browser = (
  extra: Partial<NonNullable<Rights['browser']>> = {},
): NonNullable<Rights['browser']> => ({
  userBrowser: false,
  sites: {},
  requests: {},
  ...extra,
})

test('every field of Rights is compared by widens, and none is added without it', () => {
  // The fields come from the binding Rust exports, so a new field fails here until `widens` knows it.
  const source = readFileSync(
    new URL('../src/types/bindings/Rights.ts', import.meta.url),
    'utf8',
  )
  const body = source.slice(source.indexOf('export type Rights'))
  const fields = [...body.matchAll(/\b([a-z][A-Za-z]*)\??:/g)].map(
    (match) => match[1],
  )
  assert.deepEqual(
    [...new Set(fields)].sort(),
    [...COMPARED_RIGHTS, ...UNCONFIRMED_RIGHTS].sort(),
    'Rights has a field widens neither compares nor leaves out on purpose',
  )
})

test('the fields of the browser and password rights are compared as well', () => {
  const read = (name: string) =>
    readFileSync(
      new URL(`../src/types/bindings/${name}.ts`, import.meta.url),
      'utf8',
    )
  const fieldsOf = (name: string) =>
    [
      ...read(name)
        .slice(read(name).indexOf('export type'))
        .matchAll(/\b([a-z][A-Za-z]*)\??:/g),
    ]
      .map((match) => match[1])
      .sort()
  // `widens` reads exactly these; a new field has to be handled there first.
  assert.deepEqual(fieldsOf('BrowserRights'), [
    'all',
    'requests',
    'sites',
    'userBrowser',
  ])
  assert.deepEqual(fieldsOf('PasswordRights'), ['defaultTag', 'level', 'tags'])
  assert.deepEqual(fieldsOf('SiteGrant'), ['fillRemembered', 'level'])
})

test('nothing changed, or less, is no widening', () => {
  assert.equal(widens(NOTHING, NOTHING), false)
  const full = rights({
    operate: { 'wm.read': 'write', 'chat.read': 'read' },
    browser: browser({
      userBrowser: true,
      all: 'operate',
      sites: {
        'https://a.example': { level: 'operate', fillRemembered: true },
      },
      requests: { 'https://t.example': 'allow' },
    }),
    passwords: { level: 'readWrite', tags: null, defaultTag: null },
    extensions: ['a', 'b'],
    allExtensions: true,
    commands: true,
  })
  assert.equal(widens(full, full), false)
  assert.equal(widens(full, NOTHING), false)
  assert.equal(widens(full, rights({ operate: { 'wm.read': 'read' } })), false)
})

test('a topic that is new or higher widens, a lower one does not', () => {
  assert.equal(
    widens(NOTHING, rights({ operate: { 'wm.read': 'read' } })),
    true,
  )
  assert.equal(
    widens(
      rights({ operate: { 'wm.read': 'read' } }),
      rights({ operate: { 'wm.read': 'write' } }),
    ),
    true,
  )
  assert.equal(
    widens(
      rights({ operate: { 'wm.read': 'write' } }),
      rights({ operate: { 'wm.read': 'read' } }),
    ),
    false,
  )
})

test('extensions and commands never ask', () => {
  assert.equal(widens(NOTHING, rights({ extensions: ['a'] })), false)
  assert.equal(widens(NOTHING, rights({ allExtensions: true })), false)
  assert.equal(
    widens(rights({ extensions: ['a'] }), rights({ allExtensions: true })),
    false,
  )
  assert.equal(widens(NOTHING, rights({ commands: true })), false)
})

test('passwords widen by level, by reach and by a new default tag', () => {
  const pw = (
    level: 'read' | 'readWrite',
    tags: string[] | null,
    defaultTag: string | null = null,
  ) => rights({ passwords: { level, tags, defaultTag } })
  assert.equal(widens(NOTHING, pw('read', ['dev'])), true, 'the group is new')
  assert.equal(
    widens(pw('read', ['dev']), pw('readWrite', ['dev'], 'dev')),
    true,
  )
  assert.equal(
    widens(pw('read', ['dev']), pw('read', null)),
    true,
    'all entries',
  )
  assert.equal(widens(pw('read', ['dev']), pw('read', ['dev', 'bank'])), true)
  assert.equal(
    widens(
      pw('readWrite', ['dev', 'bank'], 'dev'),
      pw('readWrite', ['dev', 'bank'], 'bank'),
    ),
    true,
    'a new default tag',
  )
  assert.equal(widens(pw('read', null), pw('read', ['dev'])), false)
  assert.equal(
    widens(pw('readWrite', ['dev'], 'dev'), pw('read', ['dev'])),
    false,
  )
  assert.equal(widens(pw('read', ['dev', 'bank']), pw('read', ['dev'])), false)
  assert.equal(widens(pw('read', ['dev']), NOTHING), false)
})

test('the browser widens by switch, level, website, unblocking, fills and allowing rules', () => {
  const site = (
    level: 'read' | 'operate' | 'blocked',
    fillRemembered = false,
  ) => ({ level, fillRemembered })
  assert.equal(widens(NOTHING, rights({ browser: browser() })), true, 'on')
  assert.equal(
    widens(
      rights({ browser: browser() }),
      rights({ browser: browser({ userBrowser: true }) }),
    ),
    true,
  )
  assert.equal(
    widens(
      rights({ browser: browser() }),
      rights({ browser: browser({ all: 'read' }) }),
    ),
    true,
    'a level for all websites',
  )
  assert.equal(
    widens(
      rights({ browser: browser({ all: 'read' }) }),
      rights({ browser: browser({ all: 'operate' }) }),
    ),
    true,
  )
  assert.equal(
    widens(
      rights({ browser: browser({ all: 'operate' }) }),
      rights({ browser: browser({ all: 'read' }) }),
    ),
    false,
  )
  const a = 'https://a.example'
  assert.equal(
    widens(
      rights({ browser: browser() }),
      rights({ browser: browser({ sites: { [a]: site('read') } }) }),
    ),
    true,
    'a new website',
  )
  assert.equal(
    widens(
      rights({ browser: browser({ sites: { [a]: site('read') } }) }),
      rights({ browser: browser({ sites: { [a]: site('operate') } }) }),
    ),
    true,
  )
  assert.equal(
    widens(
      rights({ browser: browser({ sites: { [a]: site('blocked') } }) }),
      rights({ browser: browser({ sites: { [a]: site('read') } }) }),
    ),
    true,
    'unblocked',
  )
  assert.equal(
    widens(
      rights({
        browser: browser({ all: 'operate', sites: { [a]: site('blocked') } }),
      }),
      rights({ browser: browser({ all: 'operate' }) }),
    ),
    true,
    'a blocked website removed from the list',
  )
  assert.equal(
    widens(
      rights({ browser: browser({ all: 'operate' }) }),
      rights({
        browser: browser({ all: 'operate', sites: { [a]: site('blocked') } }),
      }),
    ),
    false,
    'a new block narrows',
  )
  assert.equal(
    widens(
      rights({ browser: browser({ sites: { [a]: site('read') } }) }),
      rights({ browser: browser({ sites: { [a]: site('read', true) } }) }),
    ),
    true,
    'a fill that asks no more',
  )
  const t = 'https://t.example'
  assert.equal(
    widens(
      rights({ browser: browser() }),
      rights({ browser: browser({ requests: { [t]: 'allow' } }) }),
    ),
    true,
    'a new allowing rule',
  )
  assert.equal(
    widens(
      rights({ browser: browser({ requests: { [t]: 'deny' } }) }),
      rights({ browser: browser({ requests: { [t]: 'allow' } }) }),
    ),
    true,
    'a rule turned to allow',
  )
  assert.equal(
    widens(
      rights({ browser: browser({ requests: { [t]: 'allow' } }) }),
      rights({ browser: browser({ requests: { [t]: 'deny' } }) }),
    ),
    false,
  )
  assert.equal(
    widens(rights({ browser: browser({ userBrowser: true }) }), NOTHING),
    false,
    'switching off narrows',
  )
})

test('all six profile actions are guardrails no agent can call (FR-016)', () => {
  const own = ALL_ACTIONS.filter((a) =>
    a.id.startsWith('settings.agents.profiles.'),
  )
  assert.deepEqual(own.map((a) => a.id).sort(), [
    'settings.agents.profiles.activate',
    'settings.agents.profiles.assign',
    'settings.agents.profiles.create',
    'settings.agents.profiles.delete',
    'settings.agents.profiles.rename',
    'settings.agents.profiles.rights.set',
  ])
  for (const action of own) {
    assert.equal(action.scope, 'guardrails', action.id)
    assert.equal(action.agentCallable, false, action.id)
    assert.equal(action.binding, 'global', action.id)
  }
  assert.equal(
    own.find((a) => a.id === 'settings.agents.profiles.delete')?.effect,
    'destructive',
  )
})

test('the removed per-agent rights action is gone', () => {
  assert.equal(
    ALL_ACTIONS.some((a) => a.id === 'settings.agents.mcp.grants.set'),
    false,
  )
})

test('"Standard" is translated by its id and any other profile shows its name', () => {
  const t = (key: string) => `t:${key}`
  assert.equal(
    profileLabel(t, STANDARD_PROFILE_ID, 'anything'),
    't:agentProfiles.standard',
  )
  assert.equal(
    profileLabel(t, '0b7d3c62-9f2e-4c1a-8d55-2a4b6e1f0c99', 'Strikt'),
    'Strikt',
  )
  assert.equal(kindLabelKey('external'), 'agentProfiles.kind.external')
  assert.equal(
    kindLabelKey('claudeCodeDelegate'),
    'agentProfiles.kind.subscription',
  )
  assert.equal(kindLabelKey('codexDelegate'), 'agentProfiles.kind.subscription')
  assert.equal(kindLabelKey('builtin'), 'agentProfiles.kind.builtin')
})

test('a refused profile command is told by the code in its reason', () => {
  for (const code of [
    'name_empty',
    'standard_fixed',
    'profile_missing',
    'subject_unknown',
  ]) {
    assert.equal(profileErrorCode({ kind: 'InvalidInput', reason: code }), code)
  }
  assert.equal(
    profileErrorCode({ kind: 'InvalidInput', reason: 'no such agent' }),
    null,
  )
  assert.equal(profileErrorCode({ kind: 'Io', reason: 'name_empty' }), null)
  assert.equal(profileErrorCode('name_empty'), null)
})

test('both languages have the texts of every profile code and kind', () => {
  for (const language of ['de', 'en']) {
    const texts = JSON.parse(
      readFileSync(
        new URL(`../src/i18n/locales/${language}.json`, import.meta.url),
        'utf8',
      ),
    ).agentProfiles
    for (const code of [
      'name_empty',
      'standard_fixed',
      'profile_missing',
      'subject_unknown',
    ]) {
      assert.ok(texts.errors[code], `${language}: ${code}`)
    }
    for (const kind of ['external', 'subscription', 'builtin']) {
      assert.ok(texts.kind[kind], `${language}: ${kind}`)
    }
    assert.ok(texts.standard, `${language}: standard`)
  }
})

test('a topic of the notice is named by its group, one that is no topic has no name', () => {
  assert.equal(topicKeyOf('chat.write'), 'mcpAgents.rights.groups.chat.title')
  assert.equal(topicKeyOf('nonsense'), null)
})
