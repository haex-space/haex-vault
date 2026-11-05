// Part of `pnpm check:agent-actions` (spec 082 US5, T028): every failed `runAction` outcome
// carries one sentence on what an agent should do next, beside its unchanged message; a
// `needs_choice` keeps its candidates and has none.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { toOutcomeWire } from '../src/lib/actions/agentTools.ts'
import {
  ActionChoiceError,
  createActionRunner,
  type ActionHandler,
  type ActionRunnerDeps,
} from '../src/lib/actions/runner.ts'
import type {
  ActionCaller,
  ActionDefinition,
  ActionOutcome,
} from '../src/lib/actions/types.ts'

const AGENT: ActionCaller = { kind: 'builtinAgent' }

function action(
  overrides: Partial<ActionDefinition> & { id: string },
): ActionDefinition {
  return {
    titleKey: `actions.${overrides.id}`,
    description: 'test action',
    input: { type: 'object', properties: {} },
    result: { type: 'object' },
    target: 'none',
    scope: 'wm.layout',
    effect: 'write',
    agentCallable: true,
    binding: 'global',
    ...overrides,
  }
}

const NOTE = action({
  id: 'test.note',
  input: {
    type: 'object',
    properties: { title: { type: 'string' } },
    required: ['title'],
  },
})
const CLOSE = action({
  id: 'test.close',
  target: 'tab',
  input: { type: 'object', properties: { tabId: { type: 'string' } } },
})
const LOCKED = action({ id: 'test.locked', agentCallable: false })
const IN_APP = action({ id: 'test.in.app', binding: 'tab', appId: 'x' })
const BROKEN = action({ id: 'test.broken' })
const UNHANDLED = action({ id: 'test.unhandled' })
const AMBIGUOUS = action({ id: 'test.ambiguous' })

const handlers = new Map<string, ActionHandler>([
  [NOTE.id, () => ({})],
  [CLOSE.id, () => ({})],
  [
    BROKEN.id,
    () => {
      throw new Error('boom')
    },
  ],
  [
    AMBIGUOUS.id,
    () => {
      throw new ActionChoiceError('which app?', 'appId', [
        { value: 'a', label: 'A' },
        { value: 'b', label: 'B' },
      ])
    },
  ],
])

const deps: ActionRunnerDeps = {
  catalog: [NOTE, CLOSE, LOCKED, IN_APP, BROKEN, UNHANDLED, AMBIGUOUS],
  globalHandler: (id) => handlers.get(id),
  resolveFocus: () => null,
  targetExists: (_target, id) => id === 'tab-1',
  awaitTabHandler: async () => null,
}

const run = (id: string, input: Record<string, unknown> = {}) =>
  createActionRunner(deps).runAction(id, input, AGENT)

function failed(outcome: ActionOutcome) {
  assert.equal(outcome.ok, false)
  if (outcome.ok) throw new Error('unreachable')
  return outcome
}

test('every error code ends with a next step, the message unchanged', async () => {
  const cases: [string, Promise<ActionOutcome>, RegExp][] = [
    ['unknown_action', run('test.nope'), /find_actions/],
    ['invalid_input', run(NOTE.id, {}), /`title`/],
    ['target_required', run(CLOSE.id), /`tabId`.*wm_state_get/],
    ['target_not_found', run(CLOSE.id, { tabId: 'tab-9' }), /wm_state_get/],
    ['forbidden_for_agents', run(LOCKED.id), /tell the user/],
    ['app_unavailable', run(IN_APP.id), /not ready in time/],
    ['failed', run(BROKEN.id), /did not work/],
    ['failed', run(UNHANDLED.id), /did not work/],
  ]
  for (const [code, pending, next] of cases) {
    const outcome = failed(await pending)
    assert.equal(outcome.code, code)
    assert.ok(outcome.message.length > 0, code)
    assert.ok(
      outcome.next !== undefined && !outcome.message.includes(outcome.next),
      code,
    )
    assert.match(outcome.next, next, code)
  }
})

test('the message of each code stays what it was', async () => {
  assert.equal(
    failed(await run('test.nope')).message,
    'unknown action test.nope',
  )
  assert.equal(failed(await run(BROKEN.id)).message, 'boom')
})

test('needs_choice keeps its candidates and has no next step', async () => {
  const outcome = failed(await run(AMBIGUOUS.id))
  assert.equal(outcome.code, 'needs_choice')
  assert.equal(outcome.next, undefined)
  assert.deepEqual(
    outcome.options?.map((o) => o.value),
    ['a', 'b'],
  )
})

test('the next step travels to Rust; an outcome without one stays as it was', async () => {
  const wire = toOutcomeWire(await run(NOTE.id, {}))
  assert.ok(!wire.ok && wire.next?.includes('`title`'))
  assert.deepEqual(
    Object.keys(
      toOutcomeWire({ ok: false, code: 'failed', message: 'x' }),
    ).sort(),
    ['code', 'message', 'ok'],
  )
})
