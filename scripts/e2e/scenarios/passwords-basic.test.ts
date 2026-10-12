import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { clearButtonOf } from '../lib/fields.ts'
import {
  createEntry,
  entryTitles,
  openPasswords,
  overview,
  restoreEntry,
  trashEntry,
} from '../lib/passwords.ts'

const MARKER = 'SECRET-MARKER-E2E-PASSWORDS'

// Spec 034, quickstart §3 to §5 (US1, US2): one instance, the password manager window. An entry made in
// the editor shows in the list, the search finds and misses it, its TOTP code has the shape of a code
// and a copy answers with the time the clipboard is kept; trashing moves it out of the list and
// restoring brings it back. Nothing the scenario prints holds the planted password.
scenario('passwords-basic', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-passwords' })
  await openPasswords(instance)
  ctx.step('opened')

  // The editor: title, user name, password and a TOTP secret, then save.
  await instance.click('passwords-new')
  await instance.click('passwords-new-entry')
  await instance.waitForDisplayed('passwords-editor')
  await instance.type('passwords-field-title', 'GitHub')
  await instance.type('passwords-field-username', 'octo')
  await instance.type('passwords-field-password', MARKER)
  await instance.type('passwords-field-otp', 'JBSWY3DPEHPK3PXP')
  await instance.click('passwords-editor-save')
  await instance.waitForDisplayed('passwords-edit')
  const id = (await overview(instance)).headers[0]?.id
  assert.ok(id, 'the entry exists in the overview')
  assert.deepEqual(await entryTitles(instance), ['GitHub'])
  ctx.step('created in the editor')

  // An open entry shows its breadcrumbs above it, and the toolbar drops search and "+" (spec 036,
  // FR-010a).
  await instance.waitForDisplayed('passwords-crumb-root')
  assert.equal(
    await instance.exec<boolean>(
      `return document.querySelector('[data-testid="passwords-search"], [data-testid="passwords-new"]') === null`,
    ),
    true,
    'search and "+" leave the toolbar of an open entry',
  )
  ctx.step('entry breadcrumbs')

  // The search shows the entry for a word of its title and nothing for a word that is nowhere.
  await instance.click('passwords-crumb-root')
  await instance.waitForDisplayed(`passwords-entry-${id}`)
  await instance.type('passwords-search', 'zzz-nothing')
  await instance.waitForDisplayed('passwords-empty')
  await instance.click(clearButtonOf('passwords-search'))
  await instance.waitForDisplayed(`passwords-entry-${id}`)
  await instance.type('passwords-search', 'git')
  await instance.waitForDisplayed(`passwords-entry-${id}`)
  await instance.click(clearButtonOf('passwords-search'))
  ctx.step('searched')

  // The TOTP code of the entry has six digits and the time left; the backend computes it.
  const code = unwrap<{ code: string; remainingSeconds: number }>(
    'passwords_totp_code',
    await instance.invoke('passwords_totp_code', { args: { itemId: id } }),
  )
  assert.match(code.code, /^\d{6}$/)
  assert.ok(code.remainingSeconds >= 1 && code.remainingSeconds <= 30)
  await instance.click(`passwords-entry-${id}`)
  await instance.waitForDisplayed('passwords-totp-code')
  ctx.step('totp code shown')

  // A copy goes through the backend; the answer names the time the clipboard is kept, never the value.
  const copied = unwrap<{ clearsInSeconds: number | null }>(
    'passwords_copy_field',
    await instance.invoke('passwords_copy_field', {
      args: { itemId: id, field: { kind: 'password' } },
    }),
  )
  assert.ok(
    copied.clearsInSeconds === null || copied.clearsInSeconds > 0,
    `unexpected clearing time ${copied.clearsInSeconds}`,
  )
  assert.ok(!JSON.stringify(copied).includes(MARKER))
  ctx.step('copied')

  // Trash and restore: the entry leaves the list's titles in the trash and returns.
  await trashEntry(instance, id)
  await ctx.waitFor(
    'the entry to sit in the trash',
    async () =>
      (await overview(instance)).headers.find((h) => h.id === id)?.groupId ===
      'trash',
  )
  await restoreEntry(instance, id)
  await ctx.waitFor(
    'the entry to leave the trash',
    async () =>
      (await overview(instance)).headers.find((h) => h.id === id)?.groupId !==
      'trash',
  )
  assert.deepEqual(await entryTitles(instance), ['GitHub'])
  // Entries made through the command are found like any other.
  await createEntry(instance, { title: 'Second' })
  assert.deepEqual(await entryTitles(instance), ['GitHub', 'Second'])
  ctx.step('trashed and restored')
})
