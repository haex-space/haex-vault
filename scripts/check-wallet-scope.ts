// Part of `pnpm check:wallet` (spec 092-wallet-passes, Phase W): which app shows an entry. The
// password manager shows logins and unknown types, Wallet the passes; the tags and the emptying of
// the one trash follow the same cut (src/lib/passwords/scope.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  deletePlan,
  emptyPlan,
  entryTarget,
  inScope,
  loginTagsOf,
  scopeOf,
} from '../src/lib/passwords/scope.ts'

const header = (
  id: string,
  kind: string | null,
  tagIds: string[] = [],
  groupId: string | null = null,
) => ({ id, kind, groupId, tags: tagIds.map((tag) => ({ id: tag })) })

const HEADERS = [
  header('l1', null, ['work']),
  header('l2', 'login', ['work', 'shop']),
  header('u1', 'sshkey'),
  header('p1', 'pass', ['ticket', 'work']),
  header('p2', 'pass', ['ticket']),
]

test("a pass is Wallet's; a login, a header from before types and an unknown type stay in the password manager", () => {
  assert.equal(scopeOf(HEADERS[0]), 'login')
  assert.equal(scopeOf(HEADERS[1]), 'login')
  assert.equal(scopeOf(HEADERS[2]), 'login')
  assert.equal(scopeOf(HEADERS[3]), 'pass')
  assert.equal(scopeOf(null), 'login')
  assert.deepEqual(
    inScope(HEADERS, 'login').map((h) => h.id),
    ['l1', 'l2', 'u1'],
  )
  assert.deepEqual(
    inScope(HEADERS, 'pass').map((h) => h.id),
    ['p1', 'p2'],
  )
})

test('the tags of the password manager are counted over its logins and leave out tags only passes carry', () => {
  const tags = [
    { ids: ['work'], itemCount: 3 },
    { ids: ['shop'], itemCount: 1 },
    { ids: ['ticket'], itemCount: 2 },
    { ids: ['unused'], itemCount: 0 },
  ]
  assert.deepEqual(loginTagsOf(tags, HEADERS), [
    { ids: ['work'], itemCount: 2 },
    { ids: ['shop'], itemCount: 1 },
    { ids: ['unused'], itemCount: 0 },
  ])
})

const TRASH_GROUPS = [
  { id: 'trash', parentId: null },
  { id: 'f1', parentId: 'trash' },
  { id: 'f2', parentId: 'f1' },
  { id: 'f3', parentId: 'trash' },
  { id: 'f4', parentId: 'f3' },
]
const TRASH_HEADERS = [
  header('l1', null, [], 'trash'),
  header('p1', 'pass', [], 'trash'),
  header('l2', null, [], 'f1'),
  header('p2', 'pass', [], 'f2'),
  header('l3', null, [], null),
  header('l4', null, [], 'f4'),
]

test("each app empties only its own part of the trash, a pass in a deleted folder is Wallet's", () => {
  // f1 holds a pass (below f2): it stays and only its login goes; f3 holds logins only and goes whole.
  assert.deepEqual(emptyPlan('login', 'trash', TRASH_GROUPS, TRASH_HEADERS), {
    targets: [
      { kind: 'group', id: 'f3' },
      { kind: 'item', id: 'l1' },
      { kind: 'item', id: 'l2' },
    ],
    entries: 3,
    folders: 2,
    itemIds: ['l1', 'l2', 'l4'],
  })
  assert.deepEqual(emptyPlan('pass', 'trash', TRASH_GROUPS, TRASH_HEADERS), {
    targets: [
      { kind: 'item', id: 'p1' },
      { kind: 'item', id: 'p2' },
    ],
    entries: 2,
    folders: 0,
    itemIds: ['p1', 'p2'],
  })
})

test('deleting one row for good never takes a pass with a login folder', () => {
  assert.deepEqual(
    deletePlan(
      'login',
      { kind: 'group', id: 'f3' },
      TRASH_GROUPS,
      TRASH_HEADERS,
    ),
    { targets: [{ kind: 'group', id: 'f3' }], itemIds: ['l4'] },
  )
  assert.deepEqual(
    deletePlan(
      'login',
      { kind: 'group', id: 'f1' },
      TRASH_GROUPS,
      TRASH_HEADERS,
    ),
    { targets: [{ kind: 'item', id: 'l2' }], itemIds: ['l2'] },
  )
  assert.deepEqual(
    deletePlan(
      'pass',
      { kind: 'group', id: 'f1' },
      TRASH_GROUPS,
      TRASH_HEADERS,
    ),
    { targets: [], itemIds: [] },
  )
  assert.deepEqual(
    deletePlan('pass', { kind: 'item', id: 'p2' }, TRASH_GROUPS, TRASH_HEADERS),
    { targets: [{ kind: 'item', id: 'p2' }], itemIds: ['p2'] },
  )
})

test('an entry opens where it lives, and only from an opaque id', () => {
  assert.deepEqual(entryTarget(HEADERS[3]), {
    appId: 'system.wallet',
    path: '/entry/p1',
  })
  assert.deepEqual(entryTarget(HEADERS[0]), {
    appId: 'system.passwords',
    path: '/entry/l1',
  })
  assert.deepEqual(entryTarget(HEADERS[2]), {
    appId: 'system.passwords',
    path: '/entry/u1',
  })
  assert.equal(entryTarget(null), null)
  assert.equal(entryTarget(header('SECRET login', 'pass')), null)
  assert.equal(entryTarget(header('../trash', null)), null)
})
