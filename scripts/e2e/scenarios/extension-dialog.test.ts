import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import {
  fixture,
  frameOf,
  inFrame,
  install,
  openFromLauncher,
  type BridgeAnswer,
} from '../lib/extensions.ts'
import { KEY } from '../lib/settings.ts'

// Spec 017, T118: a confirmation the extension asks for (`extension_dialog_confirm`) takes the
// keyboard from the frame, which turns inert, so Enter or Escape answer it and cannot ask again
// from the frame; afterwards the keyboard is back in the frame.
scenario('extension-dialog', { timeoutMs: 180_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop'] } })
  const page = group.device('anna/laptop').page
  const probe = await install(page, fixture('e2e', 'probe'))
  await openFromLauncher(page, probe)
  await ctx.waitFor(
    'the SDK channel of the probe',
    () =>
      inFrame<boolean>(
        page,
        probe,
        `return document.documentElement.dataset.probeReady === '1'`,
      ),
    { timeoutMs: 20_000 },
  )

  const focused = () =>
    page.exec<string | null>(
      `const el = document.activeElement
       return el?.dataset.extensionId ?? el?.dataset.testid ?? null`,
    )

  /** Puts the keyboard into the frame, as clicking its delete button does, then asks without
   * waiting. (A WebDriver click inside the sandboxed frame does not come back.) */
  async function ask(): Promise<void> {
    await page.exec(`document.querySelector(arguments[0]).focus()`, [
      frameOf(probe.id),
    ])
    assert.equal(await focused(), probe.id, 'the keyboard is in the frame')
    await inFrame(
      page,
      probe,
      `window.answer = window.probe.request('extension_dialog_confirm', {
         message: 'Wirklich löschen?', destructive: true,
       }).then((a) => JSON.parse(JSON.stringify(a)))`,
    )
    await page.waitForDisplayed('extension-dialog', 10_000)
    try {
      await ctx.waitFor(
        'the confirm button to take the keyboard',
        async () => (await focused()) === 'extension-dialog-confirm',
      )
    } catch (error) {
      const at = await page.exec<string>(
        `const el = document.activeElement
         return el === null ? 'nothing' : el.outerHTML.slice(0, 200)`,
      )
      throw new Error(`${String(error)}; the keyboard is at ${at}`, {
        cause: error,
      })
    }
    assert.equal(
      await page.exec<boolean>(
        `return document.querySelector(arguments[0]).inert`,
        [frameOf(probe.id)],
      ),
      true,
      'the frame is inert while the dialog is open',
    )
  }

  async function answered(): Promise<BridgeAnswer> {
    await ctx.waitFor(
      'the dialog to close',
      async () =>
        !(await page.exec<boolean>(
          `return !!document.querySelector('[data-testid="extension-dialog"]')`,
        )),
    )
    await ctx.waitFor(
      'the keyboard back in the frame',
      async () => (await focused()) === probe.id,
    )
    return inFrame<BridgeAnswer>(page, probe, `return window.answer`)
  }

  await ask()
  await page.typeToFocused(KEY.escape)
  const cancelled = await answered()
  assert.equal(cancelled.error, undefined, JSON.stringify(cancelled))
  assert.equal(cancelled.result, false)
  ctx.step('Escape answers no')

  await ask()
  await page.typeToFocused(KEY.enter)
  const confirmed = await answered()
  assert.equal(confirmed.error, undefined, JSON.stringify(confirmed))
  assert.equal(confirmed.result, true)
  ctx.step('Enter answers yes')
})
