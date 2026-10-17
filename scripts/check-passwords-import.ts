// Part of `pnpm check:passwords` (spec 034-password-manager, US7): the pure helpers of the import
// wizard (src/lib/passwords/importReport.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  canPreviewImport,
  groupReport,
  importFailureReason,
  progressPercent,
  sourceSecrets,
} from '../src/lib/passwords/importReport.ts'

const row = (over: Record<string, unknown>) => ({
  title: 'Mail',
  folderPath: 'Work',
  kind: 'totp_invalid',
  ...over,
})

test('rows of one entry are grouped in the order they first appear', () => {
  const groups = groupReport([
    row({ itemId: 'a', kind: 'totp_invalid' }),
    row({ itemId: 'b', title: 'Bank', kind: 'icon_not_mapped' }),
    row({
      itemId: 'a',
      kind: 'attachment_too_large',
      fileName: 'x.bin',
      sizeMib: 26,
    }),
  ])
  assert.deepEqual(
    groups.map((g) => [g.itemId, g.rows.map((r) => r.kind)]),
    [
      ['a', ['totp_invalid', 'attachment_too_large']],
      ['b', ['icon_not_mapped']],
    ],
  )
})

test('rows without an entry group by title and folder, the source has its own group', () => {
  const groups = groupReport([
    row({ title: '', folderPath: '', kind: 'source_setting' }),
    row({ title: 'Same', folderPath: 'A' }),
    row({ title: 'Same', folderPath: 'A', kind: 'icon_not_mapped' }),
    row({ title: 'Same', folderPath: 'B' }),
  ])
  assert.equal(groups.length, 3)
  assert.equal(groups[0]!.key, 'source')
  assert.equal(groups[1]!.rows.length, 2)
  assert.equal(groups[2]!.folderPath, 'B')
})

test('progress is a whole percentage within 0 and 100 and unknown without a total', () => {
  assert.equal(progressPercent(0, 10), 0)
  assert.equal(progressPercent(5, 10), 50)
  assert.equal(progressPercent(1, 3), 33)
  assert.equal(progressPercent(20, 10), 100)
  assert.equal(progressPercent(-1, 10), 0)
  assert.equal(progressPercent(1, 0), null)
  assert.equal(progressPercent(Number.NaN, 5), null)
})

test('the reason of a failed import is read from the error, nothing else is', () => {
  assert.equal(
    importFailureReason({ kind: 'PasswordsImportFailed', reason: 'cancelled' }),
    'cancelled',
  )
  assert.equal(importFailureReason({ kind: 'PasswordsImportFailed' }), null)
  assert.equal(
    importFailureReason({ kind: 'Other', reason: 'cancelled' }),
    null,
  )
  assert.equal(importFailureReason('cancelled'), null)
  assert.equal(importFailureReason(null), null)
})

test('haex-vault 2.x (Legacy) asks for its vault password and no key file, KeePass for both', () => {
  assert.deepEqual(sourceSecrets('legacyhaexvault'), {
    password: true,
    keyFile: false,
  })
  assert.deepEqual(sourceSecrets('keepass'), { password: true, keyFile: true })
  assert.deepEqual(sourceSecrets('bitwarden'), {
    password: false,
    keyFile: false,
  })
})

test('the preview needs the file and what the source needs to open it', () => {
  assert.equal(canPreviewImport('legacyhaexvault', true, false, false), false)
  assert.equal(canPreviewImport('legacyhaexvault', true, false, true), false)
  assert.equal(canPreviewImport('legacyhaexvault', true, true, false), true)
  assert.equal(canPreviewImport('legacyhaexvault', false, true, false), false)
  assert.equal(canPreviewImport('keepass', true, false, true), true)
  assert.equal(canPreviewImport('keepass', true, false, false), false)
  assert.equal(canPreviewImport('lastpass', true, false, false), true)
})

test('the reasons of the haex-vault 2.x source are read like the others', () => {
  for (const reason of ['legacy_haex_vault_locked', 'no_passwords']) {
    assert.equal(
      importFailureReason({ kind: 'PasswordsImportFailed', reason }),
      reason,
    )
  }
})
