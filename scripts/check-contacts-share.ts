// Part of `pnpm check:contacts` (spec 060 FR-009 to FR-013, FR-017): the choice of the share dialog
// (src/lib/contacts/share.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  canBuild,
  selectAll,
  selectNone,
  toArgs,
  toggleCard,
  toggleDevice,
  toggleHead,
  toggleRelay,
  toggleValue,
  type ShareOptions,
} from '../src/lib/contacts/share.ts'
import {
  choose,
  defaultDecisions,
  replacing,
  summary,
  toArgs as importArgs,
  validate,
  type ImportPreview,
} from '../src/lib/contacts/shareImport.ts'

const own: ShareOptions = {
  rows: [{ id: 'a' }, { id: 'b' }, { id: 'c' }],
  head: { nickname: 'Benny', note: 'hi', organization: null },
  holzi: {
    kind: 'own',
    canSign: true,
    devices: [{ devicePubkey: 'd1' }, { devicePubkey: 'd2' }],
    relays: ['wss://one', 'wss://two'],
  },
  name: 'Ben',
  canShare: true,
}

const foreign: ShareOptions = {
  rows: [{ id: 'a' }],
  head: {},
  holzi: { kind: 'card' },
  name: '',
  canShare: true,
}

const plain: ShareOptions = {
  rows: [{ id: 'a' }],
  head: {},
  holzi: { kind: 'none' },
  name: 'Anna',
  canShare: true,
}

test('everything is chosen when the dialog opens', () => {
  const selection = selectAll(own)
  assert.deepEqual(selection.values, ['a', 'b', 'c'])
  assert.deepEqual(
    [
      selection.displayName,
      selection.nickname,
      selection.organization,
      selection.jobTitle,
      selection.note,
    ],
    [false, true, false, false, true],
    'only the head fields the contact has',
  )
  assert.deepEqual(selection.holzi, {
    kind: 'own',
    devices: ['d1', 'd2'],
    relays: ['wss://one', 'wss://two'],
  })
})

test('none clears the rows and keeps the own identity', () => {
  const selection = selectNone(own)
  assert.deepEqual(selection.values, [])
  assert.equal(selection.note, false)
  assert.deepEqual(selection.holzi, { kind: 'own', devices: [], relays: [] })
  assert.deepEqual(selectNone(foreign).holzi, { kind: 'none' })
})

test('a row toggles on its own and back', () => {
  let selection = toggleValue(selectAll(own), 'b')
  assert.deepEqual(selection.values, ['a', 'c'])
  selection = toggleValue(selection, 'b')
  assert.deepEqual([...selection.values].sort(), ['a', 'b', 'c'])
  selection = toggleHead(selection, 'note')
  assert.equal(selection.note, false)
})

test('deselecting a device or a relay removes it from the arguments', () => {
  let selection = toggleDevice(selectAll(own), 'd1')
  selection = toggleRelay(selection, 'wss://two')
  assert.deepEqual(toArgs(selection).holzi, {
    kind: 'own',
    devices: ['d2'],
    relays: ['wss://one'],
  })
})

test('the confirmed addresses of a foreign contact are one choice', () => {
  let selection = selectAll(foreign)
  assert.deepEqual(selection.holzi, { kind: 'card' })
  selection = toggleCard(selection)
  assert.deepEqual(selection.holzi, { kind: 'none' })
  selection = toggleCard(selection)
  assert.deepEqual(selection.holzi, { kind: 'card' })
  assert.deepEqual(toggleDevice(selection, 'd1').holzi, { kind: 'card' })
  assert.deepEqual(toggleRelay(selection, 'x').holzi, { kind: 'card' })
})

test('the arguments of a foreign share never carry device or relay lists', () => {
  const args = toArgs(selectAll(foreign))
  assert.deepEqual(args.holzi, { kind: 'card' })
  assert.equal(JSON.stringify(args).includes('devices'), false)
  assert.equal(JSON.stringify(args).includes('relays'), false)
})

test('a contact without addresses has no holzi choice', () => {
  assert.deepEqual(selectAll(plain).holzi, { kind: 'none' })
  assert.deepEqual(toggleCard(selectAll(plain)).holzi, { kind: 'card' })
})

test('the own card is toggled by neither the card nor the identity row', () => {
  const selection = selectAll(own)
  assert.equal(toggleCard(selection), selection)
})

test('an empty selection with an empty name cannot build', () => {
  assert.equal(canBuild(foreign, selectNone(foreign)), false)
  assert.equal(canBuild(foreign, selectAll(foreign)), true, 'the card goes')
  assert.equal(canBuild(plain, selectNone(plain)), true, 'the name goes')
  const nameless: ShareOptions = { ...plain, name: '' }
  assert.equal(canBuild(nameless, selectNone(nameless)), false)
  assert.equal(canBuild(nameless, selectAll(nameless)), true)
})

test('the own card counts only where this device can sign it', () => {
  const linked: ShareOptions = {
    ...own,
    name: '',
    rows: [],
    head: {},
    holzi: { kind: 'own', canSign: false, devices: [], relays: [] },
  }
  assert.equal(canBuild(linked, selectAll(linked)), false)
  const main: ShareOptions = { ...linked, holzi: { ...own.holzi } as never }
  assert.equal(canBuild(main, selectAll(main)), true)
})

// --- Reading a share in ---

const imported: ImportPreview = {
  target: { kind: 'contact', id: 'c1' },
  identity: { kind: 'matches' },
  blocked: false,
  items: [
    {
      id: 0,
      item: { type: 'namePart', field: 'given' },
      state: 'same',
      choices: [],
      existing: [],
    },
    {
      id: 1,
      item: { type: 'namePart', field: 'family' },
      state: 'conflict',
      choices: ['keep', 'update'],
      default: 'keep',
      existing: [],
    },
    {
      id: 2,
      item: { type: 'value' },
      state: 'new',
      choices: ['add', 'update', 'skip'],
      default: 'add',
      existing: [{ id: 'v1' }, { id: 'v2' }],
    },
    {
      id: 3,
      item: { type: 'holziAddresses' },
      state: 'new',
      choices: ['add', 'skip'],
      default: 'add',
      existing: [],
    },
  ],
}

test('the defaults add what is new and replace nothing', () => {
  const decisions = defaultDecisions(imported)
  assert.deepEqual(
    [...decisions.values()].map((d) => [d.item, d.choice]),
    [
      [1, 'keep'],
      [2, 'add'],
      [3, 'add'],
    ],
    'the same item has no decision',
  )
  assert.equal(validate(imported, decisions), null)
  assert.deepEqual(summary(imported, decisions), {
    added: 2,
    updated: 0,
    skipped: 0,
    kept: 1,
  })
})

test('an update names the value it replaces before it can be accepted', () => {
  let decisions = choose(imported, defaultDecisions(imported), 2, 'update')
  assert.equal(decisions.get(2)?.valueId, 'v1', 'starts at the first one')
  decisions = replacing(decisions, 2, 'v2')
  assert.equal(decisions.get(2)?.valueId, 'v2')
  const bare = new Map(decisions)
  bare.set(2, { item: 2, choice: 'update' })
  assert.equal(validate(imported, bare), 'needsValue')
})

test('a choice an item does not offer is ignored', () => {
  const decisions = defaultDecisions(imported)
  assert.equal(choose(imported, decisions, 3, 'update'), decisions)
  assert.equal(choose(imported, decisions, 0, 'add'), decisions)
  assert.equal(choose(imported, decisions, 9, 'add'), decisions)
})

test('the holzi addresses are added or skipped as one', () => {
  const decisions = choose(imported, defaultDecisions(imported), 3, 'skip')
  assert.equal(decisions.get(3)?.choice, 'skip')
  assert.deepEqual(
    importArgs(imported, decisions).map((d) => d.item),
    [1, 2, 3],
  )
})

test('a blocked vault or a mismatching identity cannot be accepted', () => {
  const decisions = defaultDecisions(imported)
  assert.equal(validate({ ...imported, blocked: true }, decisions), 'blocked')
  assert.equal(
    validate({ ...imported, identity: { kind: 'mismatch' } }, decisions),
    'mismatch',
  )
  assert.equal(
    validate(
      { ...imported, target: { kind: 'new' }, identity: { kind: 'mismatch' } },
      decisions,
    ),
    null,
    'a new contact takes another identity',
  )
})
