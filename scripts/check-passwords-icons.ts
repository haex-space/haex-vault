// Part of `pnpm check:passwords` (spec 034-password-manager, T088): what an `icon` value shows
// (src/lib/passwords/icons.ts). A name of the lists is a picture of haex-vault, `binary:<hash>` is a
// picture the import stored, anything else is the default of the place.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  ENTRY_ICONS,
  IMPORT_ICONS,
  resolveIcon,
  sniffImageType,
} from '../src/lib/passwords/icons.ts'

const HASH = 'a'.repeat(64)

test('a name of the picker and a name the import maps to are pictures of HaexVault', () => {
  assert.deepEqual(resolveIcon(ENTRY_ICONS[0]), {
    kind: 'name',
    name: ENTRY_ICONS[0],
  })
  assert.deepEqual(resolveIcon(IMPORT_ICONS[0]), {
    kind: 'name',
    name: IMPORT_ICONS[0],
  })
})

test('binary:<hash> is a stored picture, the hash in lower case', () => {
  assert.deepEqual(resolveIcon(`binary:${HASH}`), {
    kind: 'binary',
    hash: HASH,
  })
  assert.deepEqual(resolveIcon(`binary:${HASH.toUpperCase()}`), {
    kind: 'binary',
    hash: HASH,
  })
})

test('anything else is the default', () => {
  for (const value of [
    null,
    undefined,
    '',
    'lucide:does-not-exist',
    'mdi:linux',
    'binary:',
    'binary:not-a-hash',
    `binary:${HASH}0`,
    `x binary:${HASH}`,
  ]) {
    assert.deepEqual(resolveIcon(value), { kind: 'default' }, String(value))
  }
})

test('a picture is told apart by its first bytes and defaults to png', () => {
  assert.equal(
    sniffImageType(Uint8Array.from([0xff, 0xd8, 0xff, 0xe0])),
    'image/jpeg',
  )
  assert.equal(
    sniffImageType(Uint8Array.from([0x47, 0x49, 0x46, 0x38, 0x39])),
    'image/gif',
  )
  assert.equal(
    sniffImageType(
      Uint8Array.from([
        0x52, 0x49, 0x46, 0x46, 0, 0, 0, 0, 0x57, 0x45, 0x42, 0x50,
      ]),
    ),
    'image/webp',
  )
  assert.equal(
    sniffImageType(Uint8Array.from([0x89, 0x50, 0x4e, 0x47])),
    'image/png',
  )
  assert.equal(sniffImageType(new Uint8Array()), 'image/png')
})
