import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import type { FlowInstance } from '../lib/flows.ts'
import { deviceFiles } from '../lib/extension-files.ts'
import { overview } from '../lib/passwords.ts'
import { storeCard, trainPass } from '../lib/pkpass.ts'
import { openWallet } from '../lib/wallet.ts'

const TRAIN_SECRET = 'SECRET-MARKER-E2E-IMPORT-TRAIN'
const SHOP_SECRET = 'SECRET-MARKER-E2E-IMPORT-SHOP'

/** Puts a file on the import queue of Wallet, as the file dialog of the toolbar does. */
async function queueFile(instance: FlowInstance, path: string): Promise<void> {
  await instance.exec(
    `const store = document.querySelector('#__nuxt')?.__vue_app__?.config.globalProperties.$pinia?._s.get('walletImport')
     store.enqueue([arguments[0]])`,
    [path],
  )
}

/** The text of an input whose hook is on the input or on the field around it. */
async function inputValue(
  instance: FlowInstance,
  hook: string,
): Promise<string> {
  return await instance.exec<string>(
    `const el = document.querySelector('[data-testid="' + arguments[0] + '"]')
     const input = el?.matches('input') ? el : el?.querySelector('input')
     return input?.value ?? ''`,
    [hook],
  )
}

// Spec 092, quickstart (US6), in Wallet: a pass file is read into the dialog, which shows the pass
// and the proposal (a dated ticket gets the tag Ticket and no folder nobody made; a card without a
// date gets nothing), saving writes it, the same file again asks, skipping writes nothing, and a
// file that is no pass is refused with its reason and leaves nothing behind. No code value shows.
scenario('wallet-import', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-wallet-import' })
  await openWallet(instance)
  const files = deviceFiles('e2e-wallet-import-')
  try {
    files.write('train.pkpass', trainPass('S-1', TRAIN_SECRET))
    files.write('shop.pkpass', storeCard('C-1', SHOP_SECRET))
    files.write('notes.pkpass', 'this is not a pass')
    ctx.step('files written')

    // A dated ticket: the form shows what the file holds, the tag is proposed.
    await queueFile(instance, files.path('train.pkpass'))
    await instance.waitForDisplayed('wallet-import-form')
    const title = await inputValue(instance, 'wallet-import-title')
    assert.ok(title.includes('Example Rail'), title)
    assert.ok(title.includes('Berlin → München'), title)
    assert.equal(await inputValue(instance, 'wallet-import-tag'), 'Ticket')
    assert.equal(await inputValue(instance, 'wallet-import-folder'), '')
    await instance.waitForDisplayed('wallet-import-not-kept')
    await instance.click('wallet-import-save')
    await instance.waitForDisplayed('wallet-import-done')
    ctx.step('ticket imported')

    let data = await overview(instance)
    const ticket = data.headers.find((h) => h.title?.includes('Berlin'))
    assert.ok(ticket, 'the ticket is in the vault')
    assert.deepEqual(
      ticket.tags.map((tag) => tag.name),
      ['Ticket'],
    )
    const codes = unwrap<Array<{ value: string }>>(
      'passwords_pass_codes',
      await instance.invoke('passwords_pass_codes', {
        args: { id: ticket.id },
      }),
    )
    assert.deepEqual(
      codes.map((code) => code.value),
      [TRAIN_SECRET],
    )
    // No code value shows as text anywhere on the screen.
    assert.equal(
      await instance.exec<boolean>(
        `return document.body.innerText.includes('SECRET-MARKER-E2E-IMPORT')`,
      ),
      false,
    )
    await instance.click('wallet-import-close')

    // The same file again asks; skipping writes nothing.
    await queueFile(instance, files.path('train.pkpass'))
    await instance.waitForDisplayed('wallet-import-duplicate')
    await instance.click('wallet-import-skip')
    await ctx.waitFor(
      'the skipped import to end',
      async () =>
        await instance.exec<boolean>(
          `return document.querySelector('[data-testid="wallet-import-dialog"]') === null`,
        ),
    )
    data = await overview(instance)
    assert.equal(
      data.headers.filter((h) => h.title?.includes('Berlin')).length,
      1,
    )
    ctx.step('duplicate skipped')

    // A card without a date: no tag, no folder.
    await queueFile(instance, files.path('shop.pkpass'))
    await instance.waitForDisplayed('wallet-import-form')
    assert.equal(await inputValue(instance, 'wallet-import-tag'), '')
    await instance.click('wallet-import-save')
    await instance.waitForDisplayed('wallet-import-done')
    await instance.click('wallet-import-close')
    data = await overview(instance)
    const card = data.headers.find((h) => h.title?.includes('Corner Shop'))
    assert.ok(card, 'the card is in the vault')
    assert.equal(card.tags.length, 0)
    assert.equal(card.groupId ?? null, null)
    ctx.step('card imported')

    // A file that is no pass is refused and leaves nothing behind.
    const before = (await overview(instance)).headers.length
    await queueFile(instance, files.path('notes.pkpass'))
    await ctx.waitFor(
      'the refusal to end the import',
      async () =>
        await instance.exec<boolean>(
          `return document.querySelector('[data-testid="wallet-import-dialog"]') === null`,
        ),
    )
    assert.equal((await overview(instance)).headers.length, before)
    ctx.step('no pass refused')
  } finally {
    files.remove()
  }
})
