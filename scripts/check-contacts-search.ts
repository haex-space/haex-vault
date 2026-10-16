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

// SC-007: what the list does on each input (`components/contacts/List.vue`), over 5,000 contacts.
test('filtering and sorting 5,000 contacts takes under 200 ms per input', () => {
  const many: SearchableContact[] = Array.from({ length: 5_000 }, (_, n) => ({
    id: `c${n}`,
    name: `Vorname${n} Nachname${n}`,
    givenName: `Vorname${n}`,
    familyName: `Nachname${n}`,
    organization: n % 3 === 0 ? `Firma ${n % 50}` : null,
    phones: [`0170 ${String(n).padStart(7, '0')}`],
    emails: [`person${n}@example.org`],
  }))
  for (const query of [
    'v',
    'vorname4',
    'nachname12',
    '0170 0004',
    'Firma 7',
    'xyz',
  ]) {
    const started = performance.now()
    sortByName(filterContacts(many, query))
    const took = performance.now() - started
    assert.ok(took < 200, `${query}: ${took.toFixed(1)} ms`)
  }
})
