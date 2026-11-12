// Part of `pnpm check:passwords` (spec 036-password-redesign, FR-006, research R2): the tab of an
// entry belongs to its place. `?tab=` is secret free, `entry/:id/history` is the Verlauf tab, and an
// unknown value reads as Details.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  ENTRY_TABS,
  entryTab,
  isSecretFreeLocation,
  withEntryTab,
} from '../src/lib/passwords/registry.ts'

test('the tab query key is secret free for details and extra only', () => {
  for (const tab of ['details', 'extra']) {
    assert.ok(isSecretFreeLocation({ path: '/entry/e1', query: { tab } }), tab)
  }
  for (const tab of ['history', 'EXTRA', 'my bank', '', 'extra,details']) {
    assert.equal(
      isSecretFreeLocation({ path: '/entry/e1', query: { tab } }),
      false,
      JSON.stringify(tab),
    )
  }
})

test('tab and edit combine, and tab on another place is refused', () => {
  assert.ok(
    isSecretFreeLocation({
      path: '/entry/e1',
      query: { edit: '', tab: 'extra' },
    }),
  )
  assert.ok(
    isSecretFreeLocation({ path: '/entry/new', query: { tab: 'extra' } }),
  )
  assert.equal(
    isSecretFreeLocation({ path: '/', query: { tab: 'extra' } }),
    false,
  )
  assert.equal(
    isSecretFreeLocation({ path: '/folder/f1', query: { tab: 'extra' } }),
    false,
  )
})

test('entryTab reads the place: history path, tab query, default Details', () => {
  assert.equal(entryTab({ path: '/entry/e1/history', query: {} }), 'history')
  assert.equal(entryTab({ path: '/entry/e1', query: {} }), 'details')
  assert.equal(
    entryTab({ path: '/entry/e1', query: { tab: 'details' } }),
    'details',
  )
  assert.equal(
    entryTab({ path: '/entry/e1', query: { tab: 'extra' } }),
    'extra',
  )
  assert.equal(
    entryTab({ path: '/entry/e1', query: { tab: 'nope' } }),
    'details',
  )
  assert.equal(
    entryTab({ path: '/entry/e1', query: { tab: 'history' } }),
    'details',
  )
  assert.equal(entryTab({ path: '/', query: { tab: 'extra' } }), 'details')
  assert.equal(entryTab({ path: '/nothing', query: {} }), 'details')
})

test('a history place is never the tab of an editing entry', () => {
  assert.equal(
    entryTab({ path: '/entry/e1', query: { edit: '', tab: 'extra' } }),
    'extra',
  )
  assert.equal(
    entryTab({ path: '/entry/e1/history', query: { edit: '' } }),
    'details',
  )
})

test('withEntryTab builds the place of a tab and drops the others', () => {
  assert.deepEqual(withEntryTab('e1', 'details', {}), {
    path: '/entry/e1',
    query: {},
  })
  assert.deepEqual(withEntryTab('e1', 'extra', { edit: '' }), {
    path: '/entry/e1',
    query: { edit: '', tab: 'extra' },
  })
  assert.deepEqual(withEntryTab('e1', 'history', { tab: 'extra' }), {
    path: '/entry/e1/history',
    query: {},
  })
  for (const tab of ENTRY_TABS) {
    const place = withEntryTab('e1', tab, {})
    assert.ok(isSecretFreeLocation(place), tab)
    assert.equal(entryTab(place), tab)
  }
})
