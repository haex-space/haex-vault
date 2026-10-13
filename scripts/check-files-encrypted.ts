// Part of `pnpm check:files` (spec 048, T030): where an encrypted folder may be created, how
// encrypted entries look, and the folder menu entry.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  asksBeforeDelete,
  canCreateEncrypted,
  encryptedLook,
  shownNameKey,
} from '../src/lib/files/encrypted.ts'
import { folderMenu } from '../src/lib/files/menus.ts'

const storage = { kind: 'storage', storageId: 's1' } as const
const device = { kind: 'device' } as const

test('an encrypted folder may be created in a storage folder and on the device', () => {
  assert.equal(
    canCreateEncrypted({ source: storage, path: '/', inEncrypted: false }),
    true,
  )
  assert.equal(
    canCreateEncrypted({
      source: storage,
      path: '/Privat',
      inEncrypted: false,
    }),
    true,
  )
  assert.equal(
    canCreateEncrypted({ source: device, path: '/home/a', inEncrypted: false }),
    true,
  )
  assert.equal(
    canCreateEncrypted({
      source: device,
      path: '/home/a/.local/share/holzi',
      inEncrypted: false,
      owned: true,
    }),
    false,
  )
  assert.equal(
    canCreateEncrypted({
      source: device,
      path: '/home/a/Unterlagen',
      inEncrypted: true,
    }),
    false,
  )
  assert.equal(
    canCreateEncrypted({ source: storage, path: null, inEncrypted: false }),
    false,
  )
})

test('not inside an encrypted folder and not in an extension area', () => {
  assert.equal(
    canCreateEncrypted({
      source: storage,
      path: '/Unterlagen',
      inEncrypted: true,
    }),
    false,
  )
  for (const path of [
    '/holzi-ext',
    '/holzi-ext/v/e',
    '/holzi-ext-dev/',
    '/holzi-ext-dev/x',
  ]) {
    assert.equal(
      canCreateEncrypted({ source: storage, path, inEncrypted: false }),
      false,
      path,
    )
  }
  assert.equal(
    canCreateEncrypted({
      source: storage,
      path: '/holzi-extra',
      inEncrypted: false,
    }),
    true,
  )
})

test('the folder menu offers the encrypted folder only where allowed', () => {
  const ids = (canEncrypt: boolean) =>
    folderMenu({ readOnly: false, canPaste: false, canEncrypt }).map((entry) =>
      'id' in entry ? entry.id : '-',
    )
  assert.deepEqual(ids(true), ['newFolder', 'newEncryptedFolder', 'paste'])
  assert.deepEqual(ids(false), ['newFolder', 'paste'])
})

test('readable encrypted folders get a key icon; entries inside look ordinary', () => {
  assert.deepEqual(
    encryptedLook({
      kind: 'dir',
      encrypted: { role: 'folder', state: 'readable' },
    }),
    { icon: 'lucide:folder-key', stateKey: null },
  )
  assert.equal(
    encryptedLook({
      kind: 'file',
      encrypted: { role: 'inside', state: 'readable' },
    }),
    null,
  )
  assert.equal(encryptedLook({ kind: 'file', encrypted: null }), null)
})

test('states that cannot be read name themselves', () => {
  assert.deepEqual(
    encryptedLook({
      kind: 'dir',
      encrypted: { role: 'folder', state: 'otherVault' },
    }),
    {
      icon: 'lucide:folder-lock',
      stateKey: 'files.encrypted.state.otherVault',
    },
  )
  assert.deepEqual(
    encryptedLook({
      kind: 'file',
      encrypted: { role: 'inside', state: 'damaged' },
    }),
    { icon: 'lucide:file-warning', stateKey: 'files.encrypted.state.damaged' },
  )
  assert.equal(
    encryptedLook({
      kind: 'dir',
      encrypted: { role: 'folder', state: 'newerFormat' },
    })?.stateKey,
    'files.encrypted.state.newerFormat',
  )
})

test('unreadable encrypted entries show a word, not their random id', () => {
  assert.equal(
    shownNameKey({
      kind: 'dir',
      encrypted: { role: 'folder', state: 'readable' },
    }),
    null,
  )
  assert.equal(shownNameKey({ kind: 'file', encrypted: null }), null)
  assert.equal(
    shownNameKey({
      kind: 'dir',
      encrypted: { role: 'folder', state: 'otherVault' },
    }),
    'files.encrypted.state.otherVault',
  )
  assert.equal(
    shownNameKey({
      kind: 'file',
      encrypted: { role: 'inside', state: 'damaged' },
    }),
    'files.encrypted.damagedEntry',
  )
})

test('deleting asks first where there is no trash', () => {
  const plain = { kind: 'file', encrypted: null } as const
  const folder = {
    kind: 'dir',
    encrypted: { role: 'folder', state: 'readable', foreign: 2 },
  } as const
  const on = (input: Partial<Parameters<typeof asksBeforeDelete>[0]>) =>
    asksBeforeDelete({
      source: device,
      android: false,
      inEncrypted: false,
      entries: [plain],
      ...input,
    })
  assert.equal(on({}), false, 'a desktop moves to the trash')
  assert.equal(on({ android: true }), true)
  assert.equal(on({ source: storage }), true)
  assert.equal(on({ inEncrypted: true }), true, 'inside an encrypted folder')
  assert.equal(on({ entries: [plain, folder] }), true, 'of an encrypted folder')
})
