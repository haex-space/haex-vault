import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { createPass, overview } from '../lib/passwords.ts'
import { openWallet } from '../lib/wallet.ts'

const QR_VALUE = 'SECRET-MARKER-E2E-PASS-QR'
const BAR_VALUE = 'SECRET-MARKER-E2E-PASS-BAR'
const TYPED_VALUE = 'SECRET-MARKER-E2E-PASS-TYPED'

// Spec 092, quickstart (US2), in Wallet: a pass with a QR code and a Code 128 is made through the command and
// another one in the editor; the view draws each code, "Show large" opens the full-screen white view
// and leaving closes it, a pass whose end has passed is marked, and no code value shows as text
// anywhere on the screen (the drawing is the only place a code is seen).
scenario('wallet-basic', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-passwords-pass' })
  await openWallet(instance)
  ctx.step('opened')

  const concert = await createPass(instance, {
    title: 'Concert',
    start: { at: '2030-11-14T20:00:00+00:00', zone: 'Europe/London' },
    expiresAt: '2030-11-15',
    codes: [
      { format: 'qr', value: QR_VALUE, label: 'Entry' },
      { format: 'code128', value: BAR_VALUE },
    ],
  })
  const old = await createPass(instance, {
    title: 'Old voucher',
    passKind: 'voucher',
    expiresAt: '2020-01-01',
    codes: [{ format: 'qr', value: 'OLD-1' }],
  })
  ctx.step('passes created')

  // The overview knows the type and no code.
  const data = JSON.stringify(await overview(instance))
  assert.ok(data.includes(concert) && data.includes(old))
  assert.ok(!data.includes(QR_VALUE) && !data.includes(BAR_VALUE))
  const codes = unwrap<Array<{ id: string; format: string; value: string }>>(
    'passwords_pass_codes',
    await instance.invoke('passwords_pass_codes', { args: { id: concert } }),
  )
  assert.deepEqual(
    codes.map((code) => [code.format, code.value]),
    [
      ['qr', QR_VALUE],
      ['code128', BAR_VALUE],
    ],
  )

  // The view: both codes are drawn.
  await instance.waitForDisplayed(`wallet-pass-${concert}`)
  await instance.click(`wallet-pass-${concert}`)
  await instance.waitForDisplayed('wallet-code-0')
  await instance.waitForDisplayed('wallet-code-image-0')
  await instance.waitForDisplayed('wallet-code-image-1')
  await instance.waitForDisplayed('wallet-start')
  const shown = await instance.exec<string>(`return document.body.innerText`)
  assert.ok(
    !shown.includes(QR_VALUE) && !shown.includes(BAR_VALUE),
    'no code value is shown as text',
  )
  ctx.step('codes drawn')

  // "Show large" opens the full-screen white view; leaving closes it.
  await instance.click('wallet-show-large-0')
  await instance.waitForDisplayed('wallet-large')
  await instance.waitForDisplayed('wallet-large-code')
  assert.equal(
    await instance.exec<string>(
      `return getComputedStyle(document.querySelector('[data-testid="wallet-large"]')).backgroundColor`,
    ),
    'rgb(255, 255, 255)',
    'the ground is white whatever the theme is',
  )
  await instance.click('wallet-large-close')
  await ctx.waitFor(
    'the large view to close',
    async () =>
      await instance.exec<boolean>(
        `return document.querySelector('[data-testid="wallet-large"]') === null`,
      ),
  )
  ctx.step('shown large')

  // An expired pass is marked, a current one is not.
  assert.equal(
    await instance.exec<boolean>(
      `return document.querySelector('[data-testid="wallet-expired"]') === null`,
    ),
    true,
    'a pass that is still valid carries no expired mark',
  )
  await instance.click('wallet-back')
  await instance.waitForDisplayed(`wallet-pass-${old}`)
  await instance.click(`wallet-pass-${old}`)
  await instance.waitForDisplayed('wallet-expired')
  ctx.step('expired marked')

  // The editor: "New" opens the editor, a typed code is saved and drawn.
  await instance.click('wallet-back')
  await instance.click('wallet-new')
  await instance.waitForDisplayed('passwords-editor')
  await instance.type('passwords-field-title', 'Typed pass')
  await instance.click('wallet-code-add')
  await instance.type('wallet-code-value-0', TYPED_VALUE)
  await instance.click('passwords-editor-save')
  await instance.waitForDisplayed('wallet-code-image-0')
  const titles = (await overview(instance)).headers.map((h) => h.title)
  assert.ok(titles.includes('Typed pass'))
  ctx.step('created in the editor')
})
