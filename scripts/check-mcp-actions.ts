// `pnpm check:mcp-actions` (spec 021-mcp-server): what the frontend pushes for external agents
// and the catalog rules the MCP server relies on. Rust refuses a whole push when one definition is
// invalid, so the full list has to pass the same checks the built-in agent's share passes; the
// guardrails of the MCP access must never reach any agent (FR-016).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  TOOL_NAME_PATTERN,
  listAgentActions,
  listPushedActions,
  type TitleOf,
} from '../src/lib/actions/agentTools.ts'
import { ALL_ACTIONS } from '../src/lib/actions/catalog.ts'
import { ACTION_SCOPE_IDS } from '../src/lib/actions/scopes.ts'
import { levelsOf, rightsOf, siteOf } from '../src/lib/agents/rights.ts'

const titleOf: TitleOf = (key, locale) => `${locale}:${key}`

test('every action any agent may call is pushed, with its scope, and no guardrail', () => {
  const pushed = listPushedActions(ALL_ACTIONS, titleOf)
  const expected = ALL_ACTIONS.filter(
    (a) => a.agentCallable && a.scope !== 'guardrails',
  ).map((a) => a.id)
  assert.deepEqual(pushed.map((d) => d.actionId).sort(), expected.sort())
  for (const def of pushed) {
    assert.notEqual(def.scope, 'guardrails', def.actionId)
    assert.ok(ACTION_SCOPE_IDS.includes(def.scope), def.actionId)
  }
})

test('the push carries the built-in agent share unchanged', () => {
  const pushed = listPushedActions(ALL_ACTIONS, titleOf)
  const builtin = listAgentActions(ALL_ACTIONS, titleOf)
  assert.deepEqual(
    pushed
      .filter((d) => d.builtinAgentCallable)
      .map((d) => d.actionId)
      .sort(),
    builtin.map((d) => d.actionId).sort(),
  )
  const externalOnly = pushed
    .filter((d) => !d.builtinAgentCallable)
    .map((d) => d.actionId)
  assert.deepEqual(externalOnly.sort(), [
    'chat.message.retry',
    'chat.message.send',
    'chat.reply.cancel',
  ])
})

test('pushed tool names fit the provider pattern and are unique', () => {
  const names = listPushedActions(ALL_ACTIONS, titleOf).map((d) => d.toolName)
  for (const name of names) assert.match(name, TOOL_NAME_PATTERN, name)
  assert.equal(new Set(names).size, names.length)
})

test('the actions of the MCP access are guardrails no agent can call (FR-016)', () => {
  const own = ALL_ACTIONS.filter((a) => a.id.startsWith('settings.agents.mcp.'))
  assert.ok(own.length > 0, 'the switch exists')
  for (const action of own) {
    assert.equal(action.scope, 'guardrails', action.id)
    assert.equal(action.agentCallable, false, action.id)
  }
})

test('the scope for one secret of a password entry exists (research R13)', () => {
  assert.ok(ACTION_SCOPE_IDS.includes('passwords.secret'))
})

test('a website is written as Rust writes it, and no website is refused (spec 065)', () => {
  const cases: Array<[string, string | null]> = [
    ['https://Shop.Example.de/kasse?x=1', 'https://shop.example.de'],
    ['shop.example.de', 'https://shop.example.de'],
    ['https://shop.example.de:443', 'https://shop.example.de'],
    ['https://shop.example.de:8443/a', 'https://shop.example.de:8443'],
    ['http://intranet.example:80', 'http://intranet.example'],
    ['https://bücher.example', 'https://xn--bcher-kva.example'],
    ['file:///etc/passwd', null],
    ['javascript:alert(1)', null],
    ['https://user:pw@example.com', null],
    ['   ', null],
  ]
  for (const [typed, site] of cases) assert.equal(siteOf(typed), site, typed)
})

test('changing a scope keeps the browser rights, and an off group is left out (spec 065)', () => {
  const browser = {
    userBrowser: false,
    sites: {
      'https://shop.example.de': {
        level: 'operate' as const,
        fillRemembered: false,
      },
    },
  }
  const levels = levelsOf({ operate: { 'wm.read': 'read' } })
  assert.deepEqual(rightsOf(levels, browser), {
    operate: { 'wm.read': 'read' },
    browser,
  })
  assert.deepEqual(rightsOf(levels), { operate: { 'wm.read': 'read' } })
})
