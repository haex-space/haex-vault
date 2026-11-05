// Part of `pnpm check:agent-actions` (spec 082 US2, T033): every action the built-in agent may call
// belongs to a declared domain, no domain is empty, and what a domain also needs exists.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import {
  AGENT_DOMAINS,
  domainKeyOf,
  listAgentActions,
} from '../src/lib/actions/agentTools.ts'
import { ALL_ACTIONS } from '../src/lib/actions/catalog.ts'
import { DOMAINS_PATH, evalDomains } from './export-eval-tools.ts'

const actions = listAgentActions(ALL_ACTIONS, (key) => key)
const keys = new Set(AGENT_DOMAINS.map((d) => d.key))

test('every action of the built-in agent has a declared domain', () => {
  for (const action of actions)
    assert.ok(keys.has(domainKeyOf(action.actionId)), action.actionId)
})

test('no domain is declared without actions', () => {
  const used = new Set(actions.map((a) => domainKeyOf(a.actionId)))
  for (const domain of AGENT_DOMAINS)
    assert.ok(used.has(domain.key), domain.key)
})

test('keys and prompt names are unique, and a name is one lower-case word', () => {
  assert.equal(keys.size, AGENT_DOMAINS.length)
  const names = AGENT_DOMAINS.map((d) => d.name)
  assert.equal(new Set(names).size, names.length)
  for (const name of [...names, ...keys])
    assert.notEqual(name, 'none', 'none is the answer for no domain')
  for (const name of names) assert.match(name, /^[a-z]+$/, name)
})

test('each domain has its titles and a one-line description', () => {
  for (const domain of AGENT_DOMAINS) {
    assert.ok(domain.titles.de && domain.titles.en, domain.key)
    assert.ok(domain.description.length > 0, domain.key)
    assert.ok(!domain.description.includes('\n'), domain.key)
  }
})

test('what a domain also needs is a tool of the built-in agent', () => {
  const tools = new Set(actions.map((a) => a.toolName))
  for (const domain of AGENT_DOMAINS)
    for (const name of domain.alsoNeeds)
      assert.ok(tools.has(name), `${domain.key}: ${name}`)
})

test('the domain snapshot of the evaluation is the declaration (run pnpm export:eval-tools)', () => {
  const snapshot = JSON.parse(readFileSync(DOMAINS_PATH, 'utf8'))
  assert.deepEqual(snapshot, JSON.parse(JSON.stringify(evalDomains())))
})
