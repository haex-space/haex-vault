// Part of `pnpm check:extensions` (spec 055, T035): the effort levels, the rank and the labels of
// the models an extension may use (src/lib/extensions/modelGrants.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  EFFORT_LEVELS,
  ceilingChoices,
  groupChoice,
  groupModelsByProvider,
  allowGroup,
  startingEffort,
  effortLabelKey,
  isEffortLevel,
  placeLabelKey,
  withoutModels,
} from '../src/lib/extensions/modelGrants.ts'

test('the efforts go from none to max, and only they are levels', () => {
  assert.deepEqual(
    [...EFFORT_LEVELS],
    ['none', 'low', 'medium', 'high', 'xhigh', 'max'],
  )
  for (const level of EFFORT_LEVELS) assert.ok(isEffortLevel(level))
  for (const other of ['', 'High', 'managed', 'minimal'])
    assert.ok(!isEffortLevel(other), other)
})

test('a model offers its own efforts as ceilings, one without a choice only none', () => {
  assert.deepEqual(ceilingChoices(['max', 'low', 'high', 'medium', 'xhigh']), [
    'low',
    'medium',
    'high',
    'xhigh',
    'max',
  ])
  assert.deepEqual(ceilingChoices([]), ['none'])
  // The ceiling the extension has stays in the list, in its place.
  assert.deepEqual(ceilingChoices(['low', 'max'], 'medium'), [
    'low',
    'medium',
    'max',
  ])
})

test("a question starts with medium or the middle of the model's own levels", () => {
  assert.equal(
    startingEffort(['low', 'medium', 'high', 'xhigh', 'max']),
    'medium',
  )
  assert.equal(startingEffort(['low', 'high']), 'high')
  assert.equal(startingEffort(['low', 'high', 'max']), 'high')
  assert.equal(startingEffort(['max']), 'max')
  assert.equal(startingEffort([]), 'none')
})

test('a group counts the models the extension may use and the agents allowing all would add', () => {
  const models = [
    { id: 'a', efforts: ['low', 'high'], agent: false, ceiling: 'low' },
    { id: 'b', efforts: [], agent: true },
    {
      id: 'c',
      efforts: ['low', 'medium', 'high'],
      agent: true,
      ceiling: 'high',
    },
    { id: 'd', efforts: [], agent: false },
  ]
  assert.deepEqual(groupChoice(models), { allowed: 2, total: 4, agents: 1 })
  assert.deepEqual(groupChoice([]), { allowed: 0, total: 0, agents: 0 })
})

test('allowing a group gives each model its own starting effort and leaves the allowed ones', () => {
  const models = [
    { id: 'a', efforts: ['low', 'medium', 'high'], agent: false },
    { id: 'b', efforts: [], agent: false },
    { id: 'c', efforts: ['low', 'high'], agent: false },
    { id: 'kept', efforts: ['low', 'high'], agent: false, ceiling: 'low' },
  ]
  assert.deepEqual(allowGroup(models), [
    { modelId: 'a', effort: 'medium' },
    { modelId: 'b', effort: 'none' },
    { modelId: 'c', effort: 'high' },
  ])
})

test('effort and place have their own labels', () => {
  assert.equal(effortLabelKey('low'), 'extensions.models.effort.low')
  assert.equal(placeLabelKey('cloud'), 'extensions.models.place.cloud')
})

test('the permissions list leaves the models to their own list', () => {
  const rows = [
    { kind: 'model', target: 'a' },
    { kind: 'modelDestination', target: 'p' },
    { kind: 'web', target: 'https://a.example/*' },
  ]
  assert.deepEqual(
    withoutModels(rows).map((row) => row.kind),
    ['modelDestination', 'web'],
  )
})

test('models group by provider, those of this device first', () => {
  const models = [
    { id: '1', provider: 'Claude Code', place: 'cloud' },
    { id: '2', provider: 'Abacus AI', place: 'cloud' },
    { id: '3', provider: 'Lokal', place: 'this_device' },
    { id: '4', provider: 'Claude Code', place: 'cloud' },
  ]
  assert.deepEqual(
    groupModelsByProvider(models).map((g) => [
      g.provider,
      g.items.map((m) => m.id),
    ]),
    [
      [null, ['3']],
      ['Abacus AI', ['2']],
      ['Claude Code', ['1', '4']],
    ],
  )
  assert.deepEqual(groupModelsByProvider([]), [])
})
