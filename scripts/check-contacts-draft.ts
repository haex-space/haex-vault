// Part of `pnpm check:contacts` (spec 051, FR-003, FR-007, research R3): the editor's draft
// (src/lib/contacts/draft.ts) sends only what changed, drops empty rows and never writes a value of
// a kind this version does not know.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  draftOf,
  emptyDraft,
  newValue,
  toInput,
  toPatch,
} from '../src/lib/contacts/draft.ts'

const CONTACT = {
  id: 'c1',
  name: 'Anna Beck',
  head: { givenName: 'Anna', familyName: 'Beck', organization: 'Gärtnerei' },
  photo: null,
  origin: { kind: 'holzi', device: 'd1', caller: 'user', account: null },
  values: [
    {
      id: 'v1',
      kind: 'phone',
      label: 'mobile',
      value: '0170',
      position: 0,
      known: true,
    },
    {
      id: 'v2',
      kind: 'email',
      label: 'home',
      value: 'a@b.de',
      position: 0,
      known: true,
    },
    {
      id: 'v3',
      kind: 'future_kind',
      label: 'other',
      value: 'x',
      position: 0,
      known: false,
    },
  ],
  groups: [{ id: 'g1', name: 'Arbeit' }],
  createdAt: 't',
  updatedAt: 't',
}

test('an untouched draft is no patch', () => {
  assert.equal(toPatch(draftOf(CONTACT), draftOf(CONTACT)), null)
})

test('a changed and a cleared field, nothing else', () => {
  const draft = draftOf(CONTACT)
  draft.head.nickname = 'Anni'
  draft.head.organization = '  '
  assert.deepEqual(toPatch(draftOf(CONTACT), draft), {
    nickname: 'Anni',
    organization: null,
  })
})

test('changed, added and removed values; the unknown kind is never named', () => {
  const draft = draftOf(CONTACT)
  draft.values[0]!.value = '0171'
  draft.values = draft.values.filter((value) => value.id !== 'v2')
  draft.values.push({ ...newValue('phone'), value: '030 1' })
  draft.values.push(newValue('email'))
  const patch = toPatch(draftOf(CONTACT), draft)
  assert.deepEqual(patch?.values, {
    upsert: [
      { id: 'v1', kind: 'phone', label: 'mobile', value: '0171' },
      { kind: 'phone', label: 'mobile', value: '030 1' },
    ],
    remove: ['v2'],
  })
})

test('removing an unknown value from the draft does not delete it', () => {
  const draft = draftOf(CONTACT)
  draft.values = draft.values.filter((value) => value.id !== 'v3')
  assert.equal(toPatch(draftOf(CONTACT), draft), null)
})

test('groups go into the patch only when they changed', () => {
  const draft = draftOf(CONTACT)
  draft.groups = []
  assert.deepEqual(toPatch(draftOf(CONTACT), draft), { groups: [] })
})

test('a new contact drops empty rows and empty fields', () => {
  const draft = emptyDraft(['g1'])
  draft.head.givenName = 'Ben'
  draft.values = [{ ...newValue('phone'), value: '0170' }, newValue('email')]
  assert.deepEqual(toInput(draft), {
    head: { givenName: 'Ben' },
    values: [{ kind: 'phone', label: 'mobile', value: '0170' }],
    groups: ['g1'],
  })
})

test('a new custom value starts with an empty own label', () => {
  assert.equal(newValue('custom').label, 'custom:')
  assert.equal(newValue('event').label, 'birthday')
})
