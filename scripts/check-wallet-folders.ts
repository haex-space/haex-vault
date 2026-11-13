// Part of `pnpm check:wallet` (spec 092-wallet-passes, Phase W): Wallet's folders, the subtree of
// the folder rows under the fixed root `wallet` (src/lib/wallet/folders.ts). The password manager
// shows none of it; Wallet shows only it.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { buildTree } from '../src/lib/passwords/tree.ts'
import {
  loginGroupsOf,
  walletFoldersOf,
  walletGroupIds,
  WALLET_GROUP_ID,
} from '../src/lib/wallet/folders.ts'

const row = (
  id: string,
  parentId: string | null,
  trashedFromParentId: string | null = null,
) => ({ id, name: id, parentId, sortOrder: null, trashedFromParentId })

const GROUPS = [
  row('trash', null),
  row('wallet', null),
  row('w-work', 'wallet'),
  row('w-trains', 'w-work'),
  row('w-travel', 'wallet'),
  row('l-banks', null),
  row('l-shops', 'l-banks'),
  // A Wallet folder deleted directly lies below the trash and remembers where it came from.
  row('w-old', 'trash', 'wallet'),
  row('w-old-sub', 'w-old'),
  // A login folder deleted directly.
  row('l-old', 'trash', 'l-banks'),
]

const ids = (rows: { id: string }[]) => rows.map((r) => r.id).sort()

test('Wallet owns the root, its folders at any depth and its deleted folders', () => {
  assert.deepEqual([...walletGroupIds(GROUPS)].sort(), [
    'w-old',
    'w-old-sub',
    'w-trains',
    'w-travel',
    'w-work',
    'wallet',
  ])
})

test('the password manager keeps the trash, its own folders and its own deleted folders', () => {
  assert.deepEqual(ids(loginGroupsOf(GROUPS)), [
    'l-banks',
    'l-old',
    'l-shops',
    'trash',
  ])
})

test("Wallet's live folders leave out the root and the trash, and the top ones have no parent", () => {
  const folders = walletFoldersOf(GROUPS)
  assert.deepEqual(ids(folders), ['w-trains', 'w-travel', 'w-work'])
  assert.equal(folders.find((g) => g.id === 'w-work')?.parentId, null)
  assert.equal(folders.find((g) => g.id === 'w-trains')?.parentId, 'w-work')
  const tree = buildTree(folders, [])
  assert.deepEqual(tree.roots.map((node) => node.group.id).sort(), [
    'w-travel',
    'w-work',
  ])
  assert.deepEqual(
    tree.roots
      .find((node) => node.group.id === 'w-work')
      ?.children.map((node) => node.group.id),
    ['w-trains'],
  )
})

test("without a Wallet folder there is only the root id, and nothing is Wallet's", () => {
  const none = [row('trash', null), row('l-banks', null)]
  assert.deepEqual([...walletGroupIds(none)], [WALLET_GROUP_ID])
  assert.deepEqual(walletFoldersOf(none), [])
  assert.deepEqual(ids(loginGroupsOf(none)), ['l-banks', 'trash'])
})

test('a loop in the rows ends', () => {
  const loop = [row('a', 'b'), row('b', 'a'), row('w', 'wallet')]
  assert.deepEqual([...walletGroupIds(loop)].sort(), ['w', 'wallet'])
})
