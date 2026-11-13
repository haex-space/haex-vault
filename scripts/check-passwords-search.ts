// Part of `pnpm check:passwords` (spec 034-password-manager, FR-007, SC-002, research R11): the
// search over the headers of the entries (src/lib/passwords/search.ts). It looks at exactly title,
// username, URL and tag names, never at a note, a password or another field.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  filterHeaders,
  matchesQuery,
  placeholderParts,
  sortByTitle,
  type SearchableHeader,
} from '../src/lib/passwords/search.ts'
import { fold } from '../src/lib/search/fold.ts'

function header(
  overrides: Partial<SearchableHeader> & { id: string },
): SearchableHeader {
  return {
    title: null,
    username: null,
    url: null,
    tags: [],
    ...overrides,
  }
}

const MAIL = header({
  id: 'mail',
  title: 'Mail Account',
  username: 'alice@example.invalid',
  url: 'https://mail.example.invalid/login',
  tags: [{ id: 't-work', name: 'Arbeit' }],
})
const BANK = header({
  id: 'bank',
  title: 'Bänk Konto',
  username: 'bob',
  url: 'https://bank.example.invalid',
  tags: [{ id: 't-fin', name: 'Finanzen' }],
})

test('a word part of title, username, URL and tag names matches', () => {
  assert.ok(matchesQuery(MAIL, 'acc'), 'title part')
  assert.ok(matchesQuery(MAIL, 'alice'), 'username part')
  assert.ok(matchesQuery(MAIL, 'mail.example'), 'URL part')
  assert.ok(matchesQuery(MAIL, 'arbe'), 'tag name part')
  assert.ok(!matchesQuery(MAIL, 'bob'))
})

test('case and diacritics do not matter', () => {
  assert.ok(matchesQuery(BANK, 'BANK'))
  assert.ok(matchesQuery(BANK, 'bank'))
  assert.ok(matchesQuery(BANK, 'bänk'), 'with the umlaut')
  assert.ok(matchesQuery(BANK, 'konto'))
  assert.equal(fold('Bänk'), fold('Banke'.slice(0, 4)))
  // A decomposed umlaut finds the composed one.
  assert.ok(matchesQuery(BANK, 'bänk'))
})

test('several words must all match, in any of the fields', () => {
  assert.ok(matchesQuery(MAIL, 'mail alice'))
  assert.ok(matchesQuery(MAIL, 'arbeit login'))
  assert.ok(!matchesQuery(MAIL, 'mail bob'))
})

test('a note, a password or any other field is never searched', () => {
  const sneaky = {
    ...header({ id: 'x', title: 'Plain' }),
    note: 'SECRET-MARKER-SEARCH-note',
    password: 'SECRET-MARKER-SEARCH-password',
    keyValues: [{ key: 'PIN', value: 'SECRET-MARKER-SEARCH-pin' }],
  }
  assert.ok(!matchesQuery(sneaky, 'SECRET-MARKER-SEARCH'))
  assert.ok(!matchesQuery(sneaky, 'marker'))
  assert.deepEqual(filterHeaders([sneaky], { query: 'secret-marker' }), [])
})

test('an entry without a title is found by its username and by its URL', () => {
  const untitled = header({
    id: 'untitled',
    username: 'carol',
    url: 'https://shop.example.invalid',
  })
  assert.ok(matchesQuery(untitled, 'carol'))
  assert.ok(matchesQuery(untitled, 'shop'))
  assert.ok(
    !matchesQuery(untitled, 'untitled'),
    'the placeholder is not a value',
  )
  assert.ok(!matchesQuery(untitled, 'ohne titel'))
})

test('an empty query returns everything, a tag id filters by tag', () => {
  const all = [MAIL, BANK]
  assert.deepEqual(filterHeaders(all, { query: '' }), all)
  assert.deepEqual(filterHeaders(all, { query: '   ' }), all)
  assert.deepEqual(filterHeaders(all, { query: '', tagId: 't-fin' }), [BANK])
  assert.deepEqual(filterHeaders(all, { query: 'example', tagId: 't-work' }), [
    MAIL,
  ])
  assert.deepEqual(filterHeaders(all, { query: 'nothing' }), [])
})

test('5,000 headers are filtered well inside one second', () => {
  const many: SearchableHeader[] = Array.from({ length: 5000 }, (_, i) =>
    header({
      id: `i${i}`,
      title: `Entry ${i} Überprüfung`,
      username: `user${i}`,
      url: `https://site${i}.example.invalid`,
      tags: [{ id: `t${i % 20}`, name: `Tag ${i % 20}` }],
    }),
  )
  const started = performance.now()
  const hits = filterHeaders(many, { query: 'uberprufung entry 49' })
  const elapsed = performance.now() - started
  assert.ok(hits.length > 0)
  // A coarse sanity bound (SC-002), not a tight timing.
  assert.ok(elapsed < 1000, `took ${elapsed} ms`)
})

test('a reference placeholder is not searchable as text (spec 036)', () => {
  const id = '7c1e0000-0000-4000-8000-000000000001'
  const reference = header({
    id: 'ref',
    title: 'Zweit',
    username: `admin-{$${id}:username}`,
    url: `{$${id}:extra:a\\}b}`,
  })
  assert.equal(matchesQuery(reference, 'username'), false)
  assert.equal(matchesQuery(reference, '7c1e'), false)
  assert.equal(matchesQuery(reference, 'extra'), false)
  assert.equal(matchesQuery(reference, 'admin'), true)
  assert.equal(matchesQuery(reference, 'zweit'), true)
  // Text that only looks like a placeholder stays searchable.
  const plain = header({ id: 'plain', username: 'price {$ 5' })
  assert.equal(matchesQuery(plain, '{$'), true)
  // 36 hex digits and hyphens are no UUID unless the hyphens sit where Rust expects them.
  const notUuid = header({
    id: 'not-uuid',
    username: '{$7c1e0000000040008000-000000000001---:password}',
  })
  assert.equal(matchesQuery(notUuid, '7c1e'), true)
  // An escaped line break is part of the key, as in the Rust grammar.
  const newline = header({ id: 'newline', url: `{$${id}:extra:a\\\nb}` })
  assert.equal(matchesQuery(newline, 'extra'), false)
})

test('a list shows a placeholder as a mark, the text around it as text (spec 036)', () => {
  const id = '7c1e0000-0000-4000-8000-000000000001'
  assert.deepEqual(placeholderParts(`admin-{$${id}:username}!`), [
    { kind: 'text', text: 'admin-' },
    { kind: 'mark', text: `{$${id}:username}` },
    { kind: 'text', text: '!' },
  ])
  assert.deepEqual(placeholderParts('plain'), [{ kind: 'text', text: 'plain' }])
})

test('the list sorts by folded title, untitled entries last, ties by id', () => {
  const rows = [
    { id: 'c', title: null },
    { id: 'b', title: 'Zebra' },
    { id: 'a', title: 'zebra' },
    { id: 'd', title: 'Äpfel' },
    { id: 'e', title: '' },
    { id: 'f', title: 'Bank' },
  ]
  assert.deepEqual(
    sortByTitle(rows).map((row) => row.id),
    ['d', 'f', 'a', 'b', 'c', 'e'],
  )
})

test('a word with a typo still finds the entry, a short word must match exactly', () => {
  assert.ok(matchesQuery(MAIL, 'acount'), 'a missing letter')
  assert.ok(matchesQuery(BANK, 'finanzn'), 'a missing letter in a tag')
  assert.ok(matchesQuery(MAIL, 'arbeiz'), 'a wrong letter')
  assert.ok(
    !matchesQuery(MAIL, 'bob'),
    'three letters match exactly or not at all',
  )
  assert.ok(!matchesQuery(MAIL, 'konto'))
})

test('with a query the best hits come first, ties keep the given order', () => {
  const close = header({ id: 'close', title: 'Paypal' })
  const typo = header({ id: 'typo', title: 'Paypol' })
  const other = header({ id: 'other', title: 'Paypal' })
  assert.deepEqual(
    filterHeaders([typo, close, other], { query: 'paypal' }).map((h) => h.id),
    ['close', 'other', 'typo'],
  )
})

test('a word that leaves out letters finds the entry, if it starts like the word', () => {
  const itemis = header({
    id: 'itemis',
    title: 'Firma',
    username: 'alice@itemis.invalid',
  })
  const login = header({ id: 'login', url: 'https://login.itemis.de' })
  const github = header({ id: 'github', title: 'GitHub' })
  assert.ok(matchesQuery(header({ id: 't', title: 'itemis' }), 'itms'))
  assert.ok(matchesQuery(itemis, 'itms'), 'in the username')
  assert.ok(matchesQuery(login, 'itms'), 'in the URL')
  assert.ok(matchesQuery(github, 'gthb'))
  assert.ok(!matchesQuery(github, 'itms'), 'the letters in order, in one word')
  assert.ok(!matchesQuery(github, 'thb'), 'from the first letter of the word')
  assert.deepEqual(
    filterHeaders([github, login, itemis], { query: 'itms' }).map((h) => h.id),
    ['login', 'itemis'],
  )
})

test('folder names are searched only when the header carries them (Wallet, spec 092 FR-022)', () => {
  const ticket = header({
    id: 'ice',
    title: 'ICE 123',
    folders: ['Tickets', 'Deutsche Bahn'],
  })
  const login = header({ id: 'bank-login', title: 'Konto' })
  assert.deepEqual(
    filterHeaders([ticket, login], { query: 'bahn' }).map((h) => h.id),
    ['ice'],
    'a word of any folder of the path finds the pass',
  )
  assert.deepEqual(
    filterHeaders([ticket, login], { query: 'tickets bahn' }).map((h) => h.id),
    ['ice'],
    'the whole path matches',
  )
  // A header without `folders` (the password manager's) has none.
  assert.deepEqual(filterHeaders([login], { query: 'bahn' }), [])
  // A renamed folder takes effect at once: the names are read from the header as given.
  assert.deepEqual(
    filterHeaders([{ ...ticket, folders: ['Tickets', 'DB'] }], {
      query: 'bahn',
    }),
    [],
  )
  // A pass at the top level (no folder names) is found by its title as before.
  assert.deepEqual(
    filterHeaders([{ ...ticket, folders: [] }], { query: 'ice' }).map(
      (h) => h.id,
    ),
    ['ice'],
  )
})
