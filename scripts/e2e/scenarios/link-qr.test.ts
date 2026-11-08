import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { addThread, expectThreads } from '../lib/sync-flows.ts'
import {
  linkProgress,
  openFederation,
  openLinkedVault,
  showLinkText,
} from '../lib/sync-ui.ts'

// Spec 085, user stories 3 to 5: the main device's link text (what its QR code carries) fills the
// form of a new installation, so only the device name and the passphrase are left. The test pastes
// the text as a person does who has no camera; the scan itself needs a camera (quickstart M1 to
// M5). The main device of this group uses only the test's own servers, so the text names them, the
// form folds them to one line, and the link works without the server blocks being touched.
scenario('link-qr', { timeoutMs: 360_000 }, async (ctx) => {
  const g = await ctx.group({ users: { anna: ['laptop'] } })
  const laptop = g.device('anna/laptop')
  await addThread(laptop, 'vor dem Verknüpfen')
  await openFederation(laptop.page)
  const text = await showLinkText(laptop.page)
  const query = new URL(text).searchParams
  const shownCode = await laptop.page.exec<string>(
    `return document.querySelector('[data-testid="link-code-text"]').textContent.trim()`,
  )
  assert.equal(query.get('v'), '1')
  assert.equal(query.get('c'), shownCode.replaceAll('-', ''))
  assert.equal(query.get('n'), laptop.vaultName)
  assert.deepEqual(query.getAll('nr'), [g.relay.url])
  assert.deepEqual(query.getAll('ir'), g.deps.irohRelays ?? [])
  assert.deepEqual(query.getAll('off'), [])
  assert.deepEqual(
    [...query.keys()].sort(),
    ['c', 'n', 'nr', 'v', ...(g.deps.irohRelays?.length ? ['ir'] : [])].sort(),
    'the text contains only the expected code, name, and server fields',
  )

  const phone = g.addDevice('anna', 'phone')
  assert.ok(
    ![...query.values()].includes(phone.passphrase),
    'the known passphrase is not carried as a link field',
  )
  await phone.startUnopened()
  await phone.page.click('landing-link')
  await phone.page.waitForDisplayed('link-code')
  await phone.page.type('link-code', text)
  // Typing is read when the field is left.
  await phone.page.click('link-device-name')
  await phone.page.waitForDisplayed('link-servers-summary', 15_000)
  assert.equal(
    await phone.page.exec<boolean>(
      `return document.querySelector('[data-testid="link-servers-nostr-default"]') === null`,
    ),
    true,
    'the server blocks are folded to one line',
  )
  ctx.step('the link text filled the form and folded the servers')

  await phone.page.type('link-device-name', 'phone')
  await phone.page.type('link-vault-name', phone.vaultName)
  await phone.page.type('#link-passphrase', phone.passphrase)
  await phone.page.type('#link-passphrase-confirm', phone.passphrase)
  await phone.page.click('link-submit')
  await laptop.page.waitForDisplayed('link-confirm', 40_000)
  await laptop.page.click('link-confirm')
  await ctx.waitFor(
    'the new device to get the vault',
    async () => (await linkProgress(phone.page)) === 'done',
    { timeoutMs: 40_000, fixed: true },
  )
  await openLinkedVault(phone.page, phone.passphrase)
  await expectThreads(
    ctx,
    phone,
    ['vor dem Verknüpfen'],
    'the new device to show the data of the vault',
  )
  ctx.step('linked from the pasted text, with all of the data')
})
