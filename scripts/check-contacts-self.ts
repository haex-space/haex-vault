// Part of `pnpm check:contacts` (spec 060, FR-001 to FR-005): the own contact "Ich" reads "Ich"
// whatever its name is, stands first, and is found by its real name.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import { filterContacts, sortByName } from '../src/lib/contacts/search.ts'
import {
  SELF_CONTACT_ID,
  contactTitle,
  selfFirst,
  splitSelf,
} from '../src/lib/contacts/self.ts'

const labels = { self: 'Ich', noName: 'Ohne Namen' }

const own = (name: string) => ({
  id: SELF_CONTACT_ID,
  name,
  isSelf: true,
  givenName: name || null,
  familyName: null,
  organization: null,
  phones: [] as string[],
  emails: [] as string[],
})

const other = (id: string, name: string) => ({
  id,
  name,
  isSelf: false,
  givenName: name || null,
  familyName: null,
  organization: null,
  phones: [] as string[],
  emails: [] as string[],
})

test('the own contact reads "Ich" whatever its name is', () => {
  assert.equal(contactTitle(own('Martin Drechsel'), labels), 'Ich')
  assert.equal(contactTitle(own(''), labels), 'Ich')
})

test('another contact reads its name, or "no name"', () => {
  assert.equal(contactTitle(other('1', 'Anna Beck'), labels), 'Anna Beck')
  assert.equal(contactTitle(other('2', ''), labels), 'Ohne Namen')
})

test('the own contact is first whatever its name', () => {
  for (const name of ['Zacharias', 'Anna', '']) {
    const sorted = sortByName([
      other('1', 'Bert'),
      own(name),
      other('2', 'Carl'),
    ])
    assert.equal(selfFirst(sorted)[0]?.id, SELF_CONTACT_ID, name)
    assert.deepEqual(
      selfFirst(sorted)
        .slice(1)
        .map((contact) => contact.id),
      sorted.filter((contact) => !contact.isSelf).map((contact) => contact.id),
    )
  }
})

test('the own contact is found by its real name, not by "Ich"', () => {
  const all = [own('Martin Drechsel'), other('1', 'Anna Beck')]
  assert.deepEqual(
    filterContacts(all, 'drechsel').map((contact) => contact.id),
    [SELF_CONTACT_ID],
  )
  assert.deepEqual(filterContacts(all, 'ich'), [])
})

test('splitSelf separates the own contact and keeps the order of the rest', () => {
  const { own: found, others } = splitSelf([
    other('1', 'Bert'),
    own(''),
    other('2', 'Carl'),
  ])
  assert.equal(found?.id, SELF_CONTACT_ID)
  assert.deepEqual(
    others.map((contact) => contact.id),
    ['1', '2'],
  )
  assert.equal(splitSelf([other('1', 'Bert')]).own, null)
})

test('the id matches the one Rust seeds', () => {
  const rust = readFileSync(
    new URL('../src-tauri/src/contacts/ids.rs', import.meta.url),
    'utf8',
  )
  assert.ok(rust.includes(`"${SELF_CONTACT_ID}"`))
})
