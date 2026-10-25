// Part of `pnpm check:extensions` (spec 055, T035): the effort levels, the rank and the labels of
// the models an extension may use (src/lib/extensions/modelGrants.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  EFFORT_LEVELS,
  ceilingChoices,
  startingEffort,
  effortLabelKey,
  isEffortLevel,
  moved,
  placeLabelKey,
  rankOf,
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

test('only the models the extension may use are ranked, in the order they come', () => {
  assert.deepEqual(
    rankOf([
      { id: 'b', ceiling: 'low' },
      { id: 'a', ceiling: 'high' },
      { id: 'c' },
    ]),
    ['b', 'a'],
  )
  assert.deepEqual(rankOf([{ id: 'c' }]), [])
})

test('a model moves one place and stays at an edge', () => {
  const rank = ['a', 'b', 'c']
  assert.deepEqual(moved(rank, 'b', -1), ['b', 'a', 'c'])
  assert.deepEqual(moved(rank, 'b', 1), ['a', 'c', 'b'])
  assert.deepEqual(moved(rank, 'a', -1), ['a', 'b', 'c'])
  assert.deepEqual(moved(rank, 'c', 1), ['a', 'b', 'c'])
  assert.deepEqual(moved(rank, 'x', 1), ['a', 'b', 'c'])
  assert.deepEqual(rank, ['a', 'b', 'c'], 'the rank given stays as it was')
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
