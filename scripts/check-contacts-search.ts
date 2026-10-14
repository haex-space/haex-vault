// Part of `pnpm check:contacts` (spec 051, FR-004, research R11): the search of the address book
// (src/lib/contacts/search.ts) against the cases Rust checks too (src/lib/contacts/search-cases.json).
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import {
  filterContacts,
  sortByName,
  type SearchableContact,
} from '../src/lib/contacts/search.ts'

type Cases = {
  contacts: SearchableContact[]
  cases: { query: string; matches: string[] }[]
}

const cases = JSON.parse(
  readFileSync(
    new URL('../src/lib/contacts/search-cases.json', import.meta.url),
    'utf8',
  ),
) as Cases

for (const { query, matches } of cases.cases) {
  test(`the query ${JSON.stringify(query)} finds what Rust finds`, () => {
    assert.deepEqual(
      filterContacts(cases.contacts, query).map((contact) => contact.id),
      matches,
    )
  })
}

test('letters in a query never turn it into a digit search', () => {
  const xaver: SearchableContact = {
    id: 'x',
    name: 'Xaver',
    givenName: null,
    familyName: null,
    organization: null,
    phones: ['0170 12'],
    emails: [],
  }
  assert.equal(filterContacts([xaver], '017012').length, 1)
  assert.equal(filterContacts([xaver], 'a017012').length, 0)
})

test('the list is sorted by name with nameless contacts last', () => {
  const sorted = sortByName([
    { id: '3', name: '' },
    { id: '2', name: 'Ölbaum' },
    { id: '1', name: 'anna' },
    { id: '4', name: 'Bert' },
  ])
  assert.deepEqual(
    sorted.map((contact) => contact.id),
    ['1', '4', '2', '3'],
  )
})
