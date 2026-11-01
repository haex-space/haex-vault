// Part of `pnpm check:passwords` (spec 034-password-manager, FR-027, SC-005; spec 021 research R13):
// the actions of the password manager in the catalog (src/lib/actions/passwordsActions.ts). The
// built-in agent can search titles, tags and folders and never reaches a secret; the one action
// that reads a secret is for external agents, after a prompt per call. No action copies or
// changes a secret.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  isBuiltinAgentAction,
  listAgentActions,
} from '../src/lib/actions/agentTools.ts'
import { ALL_ACTIONS } from '../src/lib/actions/catalog.ts'
import { PASSWORDS_ACTIONS } from '../src/lib/actions/passwordsActions.ts'
import { isSchemaInSubset, validate } from '../src/lib/actions/schema.ts'
import type { JsonSchema } from '../src/lib/actions/types.ts'

const titleOf = (key: string, locale: string) => `${locale}:${key}`

const search = PASSWORDS_ACTIONS.find((a) => a.id === 'passwords.items.search')!

test('the built-in agent gets the search and never the secret', () => {
  const offered = listAgentActions(ALL_ACTIONS, titleOf).map((d) => d.actionId)
  assert.ok(offered.includes('passwords.items.search'))
  assert.ok(!offered.includes('passwords.secret.get'))
  const own = ALL_ACTIONS.filter((a) => a.id.startsWith('passwords.'))
  assert.deepEqual(
    own.map((a) => a.id),
    ['passwords.items.search', 'passwords.secret.get'],
    'no action copies or changes a secret',
  )
  assert.equal(search.scope, 'passwords.read')
  assert.equal(search.effect, 'read')
  assert.equal(search.runner, 'native')
  assert.ok(isBuiltinAgentAction(search))
})

const SECRET_NAME =
  /secret|private|password|passphrase|token|apiKey|credential/i

function propertyNames(schema: JsonSchema): string[] {
  return [
    ...Object.entries(schema.properties ?? {}).flatMap(([name, child]) => [
      name,
      ...propertyNames(child),
    ]),
    ...(schema.items ? propertyNames(schema.items) : []),
  ]
}

test('the schemas are in the supported subset and no property of the search names a secret', () => {
  for (const action of PASSWORDS_ACTIONS) {
    assert.ok(isSchemaInSubset(action.input), action.id)
    assert.ok(isSchemaInSubset(action.result), action.id)
  }
  for (const schema of [search.input, search.result]) {
    for (const name of propertyNames(schema))
      assert.doesNotMatch(name, SECRET_NAME, name)
  }
  assert.deepEqual(
    Object.keys(
      search.result.properties?.items?.items?.properties ?? {},
    ).sort(),
    ['folder', 'hasTotp', 'id', 'tags', 'title'],
    'a username and an address are not part of the answer',
  )
})

test('the input is validated: limit is an integer, unknown fields are refused', () => {
  const input = search.input
  assert.ok(validate(input, {}).ok)
  assert.ok(validate(input, { query: 'git', tag: 'work', limit: 5 }).ok)
  assert.equal(validate(input, { limit: 'many' }).ok, false)
  assert.equal(validate(input, { password: 'x' }).ok, false)
})
