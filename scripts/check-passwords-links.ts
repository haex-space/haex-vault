// Part of `pnpm check:passwords` (spec 092-wallet-passes, FR-013 to FR-016): the links between
// passes and their parents (src/lib/passwords/links.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  linkedOf,
  loopMembers,
  parentState,
  suggestParents,
  type LinkHeader,
} from '../src/lib/passwords/links.ts'

const NOW = new Date(2030, 4, 10, 12, 0, 0)
const TRASH = new Set(['trash'])

const h = (
  id: string,
  parentId: string | null = null,
  extra: Partial<LinkHeader> = {},
): LinkHeader => ({ id, title: id, kind: 'pass', parentId, ...extra })

test('no parent, an open parent, a parent in the trash and one that is not there', () => {
  const headers = [
    h('account', null, { kind: 'login' }),
    h('open', 'account'),
    h('bin', 'old', { groupId: null }),
    h('old', null, { groupId: 'trash', kind: 'login' }),
    h('lonely', 'not-synced-yet'),
    h('free'),
  ]
  const state = (id: string) =>
    parentState(
      headers.find((x) => x.id === id)!,
      headers,
      TRASH,
    )
  assert.deepEqual(state('free'), { state: 'none' })
  assert.equal(state('open').state, 'open')
  assert.equal(state('bin').state, 'in_trash')
  assert.deepEqual(state('lonely'), {
    state: 'not_available',
    parentId: 'not-synced-yet',
  })
})

test('two passes linked in opposite directions are both on a loop, a pass that leads into it is not', () => {
  const headers = [h('a', 'b'), h('b', 'a'), h('leaf', 'a'), h('own', 'own')]
  assert.deepEqual([...loopMembers(headers)].sort(), ['a', 'b', 'own'])
  const state = (id: string) =>
    parentState(
      headers.find((x) => x.id === id)!,
      headers,
      TRASH,
    ).state
  assert.equal(state('a'), 'loop')
  assert.equal(state('b'), 'loop')
  assert.equal(state('own'), 'loop')
  assert.equal(state('leaf'), 'open')
})

test('a long loop is found, and a long chain is not mistaken for one', () => {
  const ring = Array.from({ length: 50 }, (_, i) =>
    h(`r${i}`, `r${(i + 1) % 50}`),
  )
  assert.equal(loopMembers(ring).size, 50)
  const chain = Array.from({ length: 2000 }, (_, i) =>
    h(`c${i}`, i === 0 ? null : `c${i - 1}`),
  )
  assert.equal(loopMembers(chain).size, 0)
})

test('the passes of an entry are its direct children in the order of FR-021, without the trash', () => {
  const headers = [
    h('card', null, { kind: 'login' }),
    h('later', 'card', {
      startAt: new Date(2030, 5, 1, 9).toISOString(),
      startZone: '+00:00',
    }),
    h('soon', 'card', {
      startAt: new Date(2030, 4, 11, 9).toISOString(),
      startZone: '+00:00',
    }),
    h('over', 'card', {
      startAt: new Date(2030, 3, 1, 9).toISOString(),
      startZone: '+00:00',
    }),
    h('undated', 'card'),
    h('trashed', 'card', { groupId: 'trash' }),
    h('grandchild', 'soon'),
    h('other', 'elsewhere'),
  ]
  assert.deepEqual(
    linkedOf('card', headers, NOW, TRASH).map((x) => x.id),
    ['soon', 'later', 'over', 'undated'],
  )
  assert.deepEqual(linkedOf('nobody', headers, NOW, TRASH), [])
})

test('hundreds of children stay usable', () => {
  const headers = [
    h('card', null, { kind: 'login' }),
    ...Array.from({ length: 800 }, (_, i) => h(`v${i}`, 'card')),
  ]
  assert.equal(linkedOf('card', headers, NOW, TRASH).length, 800)
  assert.equal(parentState(headers[5]!, headers, TRASH).state, 'open')
})

test('the entries a pass probably belongs to come first and nothing is linked by it', () => {
  const candidates = [
    {
      ...h('other', null, { kind: 'login' }),
      title: 'Bank',
      url: 'https://bank.example',
    },
    {
      ...h('account', null, { kind: 'login' }),
      title: 'Deutsche Bahn',
      url: 'https://www.bahn.de/login',
    },
    { ...h('named', null, { kind: 'login' }), title: 'Bahn Card', url: null },
    { ...h('short', null, { kind: 'login' }), title: 'DB', url: null },
  ]
  const { probable, others } = suggestParents(
    { title: 'Bahn', url: 'bahn.de/tickets' },
    candidates,
  )
  assert.deepEqual(
    probable.map((x) => x.id),
    ['account', 'named'],
  )
  assert.deepEqual(
    others.map((x) => x.id),
    ['other', 'short'],
  )
  // A name that is too short matches nothing, and an empty pass suggests nothing.
  assert.equal(suggestParents({ title: 'DB' }, candidates).probable.length, 0)
  assert.equal(suggestParents({}, candidates).probable.length, 0)
})
