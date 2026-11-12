// Part of `pnpm check:wallet` (spec 092-wallet-passes, Phase W): which app shows an entry. The
// password manager shows logins and unknown types, Wallet the passes; the tags and the emptying of
// the one trash follow the same cut (src/lib/passwords/scope.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  emptyTargets,
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

test('each app empties only its own part of the trash', () => {
  const loginGroups = [
    { id: 'trash', parentId: null },
    { id: 'f1', parentId: 'trash' },
    { id: 'f2', parentId: 'f1' },
  ]
  const walletGroups = [
    { id: 'wallet', parentId: null },
    { id: 'w1', parentId: 'trash' },
  ]
  const headers = [
    header('l1', null, [], 'trash'),
    header('p1', 'pass', [], 'trash'),
    header('l2', null, [], 'f1'),
    header('p2', 'pass', [], 'f1'),
    header('l3', null, [], null),
  ]
  assert.deepEqual(emptyTargets('login', 'trash', loginGroups, headers), [
    { kind: 'group', id: 'f1' },
    { kind: 'item', id: 'l1' },
  ])
  assert.deepEqual(emptyTargets('pass', 'trash', walletGroups, headers), [
    { kind: 'group', id: 'w1' },
    { kind: 'item', id: 'p1' },
  ])
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
