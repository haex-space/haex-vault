// Part of `pnpm check:passwords` (spec 092-wallet-passes, FR-002, FR-003, FR-012): the draft of the
// pass editor (src/lib/passwords/passDraft.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  clonePassDraft,
  draftIndexOfPosition,
  emptyPassDraft,
  isPassDirty,
  passDraftFromDetail,
  startIsValid,
  toPassInput,
  toPassPatch,
  type CodeLike,
  type PassDetailLike,
} from '../src/lib/passwords/passDraft.ts'

/** What goes over the wire: `undefined` properties do not. */
const plain = <T>(value: T): unknown => JSON.parse(JSON.stringify(value))

const detail: PassDetailLike = {
  title: 'Concert',
  note: 'Block B',
  url: 'https://tickets.example.org',
  icon: null,
  color: null,
  expiresAt: '2026-11-15',
  passKind: 'ticket',
  startAt: '2026-11-14T20:00:00.000Z',
  startZone: 'Europe/London',
  parentId: null,
  tags: [{ name: 'Ticket' }, { name: 'Music' }],
  keyValues: [{ id: 'kv1', key: 'Seat', value: '14C' }],
}
const codes: CodeLike[] = [
  { id: 'c1', format: 'qr', value: 'VALUE-1', label: 'Entry' },
  { id: 'c2', format: 'code128', value: 'ABC-123', label: null },
]

test('an empty draft sends only the kind and empty lists', () => {
  assert.deepEqual(plain(toPassInput(emptyPassDraft())), {
    passKind: 'other',
    codes: [],
    keyValues: [],
    tagNames: [],
  })
})

test('a new pass sends the moment as an instant with the zone of its place', () => {
  const draft = emptyPassDraft()
  draft.title = 'Concert'
  draft.passKind = 'ticket'
  draft.start = {
    mode: 'moment',
    wall: '2026-11-14T20:00',
    zone: 'Europe/London',
  }
  draft.codes = [
    { id: null, format: 'qr', value: 'VALUE-1', label: ' Entry ' },
    { id: null, format: 'code128', value: '', label: '' },
    { id: null, format: 'code128', value: 'ABC-123', label: '' },
  ]
  const input = toPassInput(draft)
  assert.deepEqual(input.start, {
    at: '2026-11-14T20:00:00.000Z',
    zone: 'Europe/London',
  })
  assert.deepEqual(plain(input.codes), [
    { format: 'qr', value: 'VALUE-1', label: 'Entry', position: 0 },
    { format: 'code128', value: 'ABC-123', position: 1 },
  ])
})

test('a start with a date alone sends the date, and a start that does not exist is not sent', () => {
  const draft = emptyPassDraft()
  draft.start = { mode: 'date', date: '2026-12-24' }
  assert.deepEqual(toPassInput(draft).start, { date: '2026-12-24' })
  draft.start = { mode: 'date', date: '' }
  assert.equal(startIsValid(draft.start), false)
  assert.equal(toPassInput(draft).start, undefined)
  draft.start = { mode: 'moment', wall: '', zone: 'Europe/London' }
  assert.equal(startIsValid(draft.start), false)
  draft.start = { mode: 'moment', wall: '2026-11-14T20:00', zone: '' }
  assert.equal(startIsValid(draft.start), false)
  draft.start = { mode: 'none' }
  assert.equal(startIsValid(draft.start), true)
})

test('a stored pass opens in its own time and an unchanged draft sends nothing', () => {
  const draft = passDraftFromDetail(detail, codes)
  assert.deepEqual(draft.start, {
    mode: 'moment',
    wall: '2026-11-14T20:00',
    zone: 'Europe/London',
  })
  assert.equal(draft.codes[1]?.label, '')
  const initial = clonePassDraft(draft)
  assert.equal(isPassDirty(initial, draft), false)
  assert.deepEqual(toPassPatch(initial, draft), {})
})

test('a stored date alone opens as a date, and a moment without a zone opens as no start', () => {
  assert.deepEqual(
    passDraftFromDetail(
      { ...detail, startAt: null, startDate: '2026-12-24' },
      [],
    ).start,
    { mode: 'date', date: '2026-12-24' },
  )
  assert.deepEqual(
    passDraftFromDetail({ ...detail, startZone: null }, []).start,
    { mode: 'none' },
  )
})

test('only what the user changed goes into the update', () => {
  const initial = passDraftFromDetail(detail, codes)
  const draft = clonePassDraft(initial)
  draft.title = 'Concert 2'
  draft.expiresAt = ''
  draft.tags = ['Music', 'Ticket', 'Live']
  draft.start = {
    mode: 'moment',
    wall: '2026-11-14T21:30',
    zone: 'Europe/London',
  }
  assert.equal(isPassDirty(initial, draft), true)
  assert.deepEqual(toPassPatch(initial, draft), {
    title: 'Concert 2',
    expiresAt: null,
    start: { at: '2026-11-14T21:30:00.000Z', zone: 'Europe/London' },
    tagNames: ['Music', 'Ticket', 'Live'],
  })
})

test('clearing the start sends null, a reordered tag list sends nothing', () => {
  const initial = passDraftFromDetail(detail, codes)
  const draft = clonePassDraft(initial)
  draft.start = { mode: 'none' }
  draft.tags = ['Music', 'Ticket']
  assert.deepEqual(toPassPatch(initial, draft), { start: null })
})

test('the codes go as a list by id: a kept code keeps its id, a new one has none, a removed one is not listed', () => {
  const initial = passDraftFromDetail(detail, codes)
  const draft = clonePassDraft(initial)
  draft.codes[0]!.label = 'Gate'
  draft.codes.splice(1, 1)
  draft.codes.push({ id: null, format: 'aztec', value: 'AZTEC-1', label: '' })
  assert.deepEqual(plain(toPassPatch(initial, draft).codes), [
    { id: 'c1', format: 'qr', value: 'VALUE-1', label: 'Gate', position: 0 },
    { format: 'aztec', value: 'AZTEC-1', position: 1 },
  ])
})

test('a kind this build does not know is written back only if the user picks another', () => {
  const initial = passDraftFromDetail(
    { ...detail, passKind: 'season_ticket' },
    [],
  )
  assert.equal(initial.passKind, 'season_ticket')
  const same = clonePassDraft(initial)
  same.title = 'Renamed'
  assert.equal('passKind' in toPassPatch(initial, same), false)
  same.passKind = 'voucher'
  assert.equal(toPassPatch(initial, same).passKind, 'voucher')
})

test('free fields and the link are part of the update', () => {
  const initial = passDraftFromDetail(detail, codes)
  const draft = clonePassDraft(initial)
  draft.keyValues.push({ id: null, key: 'Gate', value: '7' })
  draft.parentId = 'parent-1'
  const patch = toPassPatch(initial, draft)
  assert.deepEqual(plain(patch.keyValues), [
    { id: 'kv1', key: 'Seat', value: '14C' },
    { key: 'Gate', value: '7' },
  ])
  assert.equal(patch.parentId, 'parent-1')
  draft.parentId = null
  draft.keyValues = initial.keyValues
  assert.deepEqual(toPassPatch(initial, draft), {})
})

test('a zone is a fixed offset or a name the platform knows', () => {
  const draft = emptyPassDraft()
  draft.start = { mode: 'moment', wall: '2026-11-14T20:00', zone: '+02:00' }
  assert.deepEqual(toPassInput(draft).start, {
    at: '2026-11-14T18:00:00.000Z',
    zone: '+02:00',
  })
  draft.start = {
    mode: 'moment',
    wall: '2026-11-14T20:00',
    zone: 'Mars/Olympus',
  }
  assert.equal(startIsValid(draft.start), false)
})

test('the position the backend names is the index among the codes that were sent', () => {
  const codes = [
    { id: null, format: 'qr', value: '', label: '' },
    { id: null, format: 'qr', value: 'A', label: '' },
    { id: null, format: 'qr', value: ' ', label: '' },
    { id: null, format: 'qr', value: 'B', label: '' },
  ]
  assert.equal(draftIndexOfPosition(codes, 0), 1)
  assert.equal(draftIndexOfPosition(codes, 1), 3)
  assert.equal(draftIndexOfPosition(codes, 2), null)
})

test('an offset outside the range is not read as UTC and saved with its text', () => {
  const draft = emptyPassDraft()
  for (const zone of ['+25:00', '+02:60', '-24:00', '+02:99']) {
    draft.start = { mode: 'moment', wall: '2026-11-14T20:00', zone }
    assert.equal(startIsValid(draft.start), false, zone)
  }
  draft.start = { mode: 'moment', wall: '2026-11-14T20:00', zone: '-05:30' }
  assert.equal(startIsValid(draft.start), true)
})

test('a date that does not exist is not a start', () => {
  const draft = emptyPassDraft()
  for (const date of ['2026-02-30', '2026-13-01', '2026-00-10', '2025-02-29']) {
    draft.start = { mode: 'date', date }
    assert.equal(startIsValid(draft.start), false, date)
  }
  for (const date of ['2026-02-28', '2028-02-29']) {
    draft.start = { mode: 'date', date }
    assert.equal(startIsValid(draft.start), true, date)
  }
})

test('emptying the value of a stored code does not delete it, an empty new one is left out', () => {
  const initial = passDraftFromDetail(detail, codes)
  const draft = clonePassDraft(initial)
  draft.codes[0]!.value = ''
  draft.codes.push({ id: null, format: 'qr', value: '', label: '' })
  // The stored code stays in the list, empty, so the backend refuses it.
  assert.deepEqual(plain(toPassPatch(initial, draft).codes), [
    { id: 'c1', format: 'qr', value: '', label: 'Entry', position: 0 },
    { id: 'c2', format: 'code128', value: 'ABC-123', position: 1 },
  ])
  assert.equal(draftIndexOfPosition(draft.codes, 0), 0)
  assert.equal(draftIndexOfPosition(draft.codes, 2), null)
})

test('a copy sends the ids of the codes, a new pass does not', () => {
  const draft = passDraftFromDetail(detail, codes)
  assert.equal(toPassInput(draft).codes[0]?.id, undefined)
  assert.equal(toPassInput(draft, { keepCodeIds: true }).codes[0]?.id, 'c1')
})
