// `pnpm check:agent-profiles` (spec 083): the catalog rules of the profile actions (all six are
// guardrails no agent can call), the labels and the error codes of profiles. Pure modules, loaded with
// Node type-stripping.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import { ALL_ACTIONS } from '../src/lib/actions/catalog.ts'
import {
  STANDARD_PROFILE_ID,
  groupByProvider,
  kindLabelKey,
  modelName,
  profileErrorCode,
  profileLabel,
} from '../src/lib/agents/profiles.ts'
import { topicKeyOf } from '../src/lib/agents/rights.ts'

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
    for (const kind of ['external', 'builtin']) {
      assert.ok(texts.kind[kind], `${language}: ${kind}`)
    }
    assert.ok(texts.standard, `${language}: standard`)
  }
})

test('a model cannot be added back to Standard while its provider has left it', () => {
  const members = readFileSync(
    new URL(
      '../src/components/settings/agentProfiles/ProfileMembers.vue',
      import.meta.url,
    ),
    'utf8',
  )
  assert.match(members, /other\.subject === subject\.providerSubject/)
  assert.match(members, /\?\.inStandard === false/)
})

test('a topic of the notice is named by its group, one that is no topic has no name', () => {
  assert.equal(topicKeyOf('chat.write'), 'mcpAgents.rights.groups.chat.title')
  assert.equal(topicKeyOf('nonsense'), null)
})

test('models are grouped by provider, the local ones first, and agents stay apart', () => {
  const model = (
    name: string,
    provider: string | null,
    local = false,
    providerSubject: string | null = provider ? `provider:${provider}` : null,
  ) => ({
    subject: `model:${name}`,
    kind: 'builtin',
    name,
    local,
    provider,
    providerSubject,
  })
  const agent = {
    subject: 'agent:probe',
    kind: 'external',
    name: 'probe',
    local: false,
    provider: null,
    providerSubject: null,
  }
  const gone = model('old', null)
  const head = {
    subject: 'provider:Codex',
    kind: 'provider',
    name: 'Codex',
    local: false,
    provider: null,
    providerSubject: null,
  }
  const { groups, rest } = groupByProvider([
    model('Opus (Claude Code)', 'Claude Code'),
    agent,
    model('Qwen', null, true),
    model('Codex 5 (Codex)', 'Codex'),
    head,
    model('Haiku (Claude Code)', 'Claude Code'),
    gone,
  ])
  assert.deepEqual(
    groups.map((g) => [g.provider, g.items.map((i) => i.name)]),
    [
      [null, ['Qwen']],
      ['Claude Code', ['Opus (Claude Code)', 'Haiku (Claude Code)']],
      ['Codex', ['Codex 5 (Codex)']],
    ],
  )
  assert.deepEqual(rest, [agent, gone], 'agents and models without a provider')
  assert.equal(
    groups[2]?.providerSubject,
    head,
    'a listed provider heads its group',
  )
  assert.equal(
    groups[1]?.providerSubject,
    null,
    'a provider that is no member is no head',
  )
  const emptyHead = { ...head, subject: 'provider:Empty', name: 'Empty' }
  const empty = groupByProvider([emptyHead]).groups[0]
  assert.equal(empty?.provider, 'Empty')
  assert.equal(empty?.providerSubject, emptyHead)
  assert.deepEqual(empty?.items, [])
  assert.deepEqual(groupByProvider([]), { groups: [], rest: [] })
})

test('a model needs no provider in its name inside the provider group', () => {
  const base = {
    subject: 'model:x',
    kind: 'builtin',
    local: false,
    providerSubject: null,
  }
  assert.equal(
    modelName({
      ...base,
      name: 'Claude Fable 5 (Claude Code)',
      provider: 'Claude Code',
    }),
    'Claude Fable 5',
  )
  assert.equal(modelName({ ...base, name: 'Qwen', provider: null }), 'Qwen')
  assert.equal(
    modelName({ ...base, name: 'Other (X)', provider: 'Y' }),
    'Other (X)',
  )
})
