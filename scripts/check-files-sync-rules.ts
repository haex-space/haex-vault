// Part of `pnpm check:files` (spec 061, T033): defaults of a new sync rule, the interval, the shown
// state, the hints of the dialog and the paths of the folder picker.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  canSave,
  childPath,
  defaultDeleteMode,
  intervalSecs,
  lastName,
  needsDecryptConsent,
  progressShare,
  parentPath,
  shownState,
  showsCloudHint,
  targetSides,
} from '../src/lib/files/syncRules.ts'

test('a new rule mirrors both ways and keeps one way (FR-005)', () => {
  assert.equal(defaultDeleteMode('both'), 'mirror')
  assert.equal(defaultDeleteMode('aToB'), 'keep')
  assert.equal(defaultDeleteMode('bToA'), 'keep')
})

test('the interval stays between one minute and a day', () => {
  assert.equal(intervalSecs(15), 900)
  assert.equal(intervalSecs(0), 60)
  assert.equal(intervalSecs(5000), 86_400)
  assert.equal(intervalSecs(Number.NaN), 900)
  assert.equal(intervalSecs(2.4), 120)
})

test('a rule shows "inactive" while the service does not run, except when paused', () => {
  assert.equal(shownState('active', true), 'active')
  assert.equal(shownState('active', false), 'inactive')
  assert.equal(shownState('paused', false), 'paused')
})

test('the dialog asks for consent and points to encrypted folders', () => {
  assert.equal(needsDecryptConsent(true, false), true)
  assert.equal(needsDecryptConsent(true, true), false)
  assert.equal(needsDecryptConsent(false, false), false)
  assert.equal(showsCloudHint(true, false), true)
  assert.equal(showsCloudHint(true, true), false)
  assert.equal(showsCloudHint(false, false), false)
})

test('a two-way rule copies into both places (FR-011)', () => {
  assert.deepEqual(targetSides('aToB'), ['b'])
  assert.deepEqual(targetSides('bToA'), ['a'])
  assert.deepEqual(targetSides('both'), ['a', 'b'])
})

test('paths of the picker go up and down on every platform', () => {
  assert.equal(parentPath('/home/anna/Fotos'), '/home/anna')
  assert.equal(parentPath('/home'), '/')
  assert.equal(parentPath('/'), null)
  assert.equal(parentPath('/Fotos/2026/'), '/Fotos')
  assert.equal(parentPath('C:\\Users\\anna'), 'C:\\Users')
  assert.equal(parentPath('C:\\Users'), 'C:\\')
  assert.equal(parentPath('C:\\'), null)
  assert.equal(childPath('/', 'Fotos'), '/Fotos')
  assert.equal(childPath('/home', 'anna'), '/home/anna')
  assert.equal(childPath('C:\\Users', 'anna'), 'C:\\Users\\anna')
  assert.equal(lastName('/home/anna'), 'anna')
  assert.equal(lastName('/'), '/')
})

test('the dialog saves only a complete rule', () => {
  const full = {
    name: 'Fotos',
    hasA: true,
    hasB: true,
    needsConsent: false,
    consent: false,
  }
  assert.equal(canSave(full), true)
  assert.equal(canSave({ ...full, name: '  ' }), false)
  assert.equal(canSave({ ...full, name: 'x'.repeat(81) }), false)
  assert.equal(canSave({ ...full, hasB: false }), false)
  assert.equal(canSave({ ...full, needsConsent: true }), false)
  assert.equal(canSave({ ...full, needsConsent: true, consent: true }), true)
})

test('the progress of a run is the share of bytes, else of files, and unknown while scanning', () => {
  const copying = { phase: 'copying' as const, filesDone: 1, filesTotal: 4 }
  assert.equal(
    progressShare({ ...copying, bytesDone: 50, bytesTotal: 200 }),
    0.25,
  )
  assert.equal(progressShare({ ...copying, bytesDone: 0, bytesTotal: 0 }), 0.25)
  assert.equal(
    progressShare({ ...copying, filesTotal: 0, bytesDone: 0, bytesTotal: 0 }),
    null,
  )
  assert.equal(
    progressShare({
      ...copying,
      phase: 'scanning',
      bytesDone: 0,
      bytesTotal: 0,
    }),
    null,
  )
  assert.equal(
    progressShare({ ...copying, bytesDone: 300, bytesTotal: 200 }),
    0.99,
  )
  assert.equal(
    progressShare({
      ...copying,
      filesDone: 4,
      bytesDone: 300,
      bytesTotal: 200,
    }),
    1,
  )
})
