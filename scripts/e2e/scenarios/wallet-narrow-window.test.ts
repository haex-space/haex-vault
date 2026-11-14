import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import type { FlowInstance } from '../lib/flows.ts'
import { deviceFiles } from '../lib/extension-files.ts'
import { clickHeaderControl } from '../lib/header.ts'
import {
  boxOverflow,
  createEntry,
  createPass,
  horizontalOverflow,
  selectTab,
} from '../lib/passwords.ts'
import { trainPass } from '../lib/pkpass.ts'
import { resizeAppWindow } from '../lib/settings.ts'
import { openWallet } from '../lib/wallet.ts'

const APP = 'system.wallet'

/** Puts a file on the import queue of Wallet, as the file dialog does. */
async function queueFile(instance: FlowInstance, path: string): Promise<void> {
  await instance.exec(
    `const store = document.querySelector('#__nuxt')?.__vue_app__?.config.globalProperties.$pinia?._s.get('walletImport')
     store.enqueue([arguments[0]])`,
    [path],
  )
}

// Spec 092, SC-010 and FR-043: at 360 px no view or dialog of Wallet scrolls sideways: the list, a
// pass with a long title and several codes and a link to a parent, each tab, the full-screen code,
// the editor, the import dialog and the capture dialog; and the controls stay reachable.
scenario('wallet-narrow-window', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-wallet-narrow' })
  const account = await createEntry(instance, { title: 'Train account' })
  const long =
    'A rather long title of a ticket that has to fit a narrow window without scrolling'
  const ticket = await createPass(instance, {
    title: long,
    url: 'https://tickets.example.invalid/a/very/long/address/of/the/provider',
    start: { at: '2030-11-14T20:00:00+00:00', zone: 'Europe/London' },
    expiresAt: '2030-11-15',
    parentId: account,
    codes: [
      {
        format: 'qr',
        value: 'SECRET-MARKER-NARROW-1',
        label: 'Outbound journey',
      },
      { format: 'code128', value: 'SECRET-MARKER-NARROW-2' },
      {
        format: 'pdf417',
        value: 'SECRET-MARKER-NARROW-3',
        label: 'Seat reservation',
      },
    ],
    keyValues: [{ key: 'Seat', value: '14C in coach 7, window side' }],
    tagNames: ['Ticket', 'a-tag-with-a-long-name-for-the-narrow-window'],
  })
  await createPass(instance, { title: 'Return trip', parentId: ticket })
  await openWallet(instance)
  await resizeAppWindow(instance, APP, 360)
  const overflow = () => horizontalOverflow(instance, '[data-wallet-app]')
  const fits = async (what: string) =>
    assert.ok((await overflow()) <= 1, `${what} scrolls sideways at 360 px`)

  await instance.waitForDisplayed(`wallet-pass-${ticket}`)
  await fits('the list')
  ctx.step('list fits')

  // The import and capture controls are in the header of the list only.
  const files = deviceFiles('e2e-wallet-narrow-')
  try {
    files.write('train.pkpass', trainPass('N-1', 'SECRET-MARKER-NARROW-IMPORT'))
    await queueFile(instance, files.path('train.pkpass'))
    await instance.waitForDisplayed('wallet-import-form')
    assert.ok(
      (await boxOverflow(instance, 'wallet-import-dialog')) <= 1,
      'the import dialog scrolls sideways at 360 px',
    )
    await instance.click('wallet-import-skip')
    await ctx.waitFor(
      'the import dialog to close',
      async () =>
        await instance.exec<boolean>(
          `return document.querySelector('[data-testid="wallet-import-dialog"]') === null`,
        ),
    )
  } finally {
    files.remove()
  }
  ctx.step('import dialog fits')

  await clickHeaderControl(instance, 'wallet-capture')
  await instance.waitForDisplayed('wallet-capture-dialog')
  assert.ok(
    (await boxOverflow(instance, 'wallet-capture-dialog')) <= 1,
    'the capture dialog scrolls sideways at 360 px',
  )
  await instance.exec<boolean>(
    `(document.activeElement ?? document).dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', keyCode: 27, bubbles: true }))
     return true`,
  )
  await ctx.waitFor(
    'the capture dialog to close',
    async () =>
      await instance.exec<boolean>(
        `return document.querySelector('[data-testid="wallet-capture-dialog"]') === null`,
      ),
  )
  ctx.step('capture dialog fits')

  await instance.click(`wallet-pass-${ticket}`)
  await instance.waitForDisplayed('wallet-code-0')
  await instance.waitForDisplayed('wallet-start')
  await fits('the pass (Details)')
  await selectTab(instance, 'extra')
  await fits('the pass (Extra)')
  await selectTab(instance, 'history')
  await fits('the pass (History)')
  await selectTab(instance, 'details')
  ctx.step('pass tabs fit')

  // The full-screen code is drawn in the window; it never scrolls sideways either.
  await instance.click('wallet-show-large-0')
  await instance.waitForDisplayed('wallet-large')
  assert.ok(
    (await boxOverflow(instance, 'wallet-large')) <= 1,
    'the large code scrolls sideways at 360 px',
  )
  await instance.click('wallet-large-close')
  ctx.step('large code fits')

  await instance.click('wallet-edit')
  await instance.waitForDisplayed('passwords-editor')
  await fits('the editor')
  await instance.click('passwords-editor-cancel')
  await instance.waitForDisplayed('wallet-title')
  ctx.step('editor fits')
})
