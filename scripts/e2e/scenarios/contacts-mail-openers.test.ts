import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { unwrap } from '../lib/flows.ts'
import { createContact, openContacts } from '../lib/contacts.ts'
import {
  fixture,
  frameOf,
  inFrame,
  install,
  type InstalledExtension,
} from '../lib/extensions.ts'
import type { FlowInstance } from '../lib/flows.ts'
import type { Page } from '../lib/page.ts'

// Spec 063, US1 (quickstart scenarios 1 to 4): the mail button of an address. Without an extension
// that writes mail it is the plain button of spec 051 (no menu); with one it opens a menu with the
// system and a row for the extension, the choice opens the extension's tab, whose page shows the
// fields of the link, and a switched-off extension is gone from the menu again. The system's mail
// program is not started here: the scenario never taps the plain button or the system row.

type Instance = Page & FlowInstance

/** `plain` for the button of spec 051, `menu` for the button that opens the choice, `none`
 * without a mail button. */
async function mailButton(page: Instance): Promise<'plain' | 'menu' | 'none'> {
  return page.exec<'plain' | 'menu' | 'none'>(
    `const el = document.querySelector('[data-testid="contacts-open-mail"]')
     if (!el) return 'none'
     return el.getAttribute('aria-haspopup') === 'menu' ? 'menu' : 'plain'`,
  )
}

async function pageRequest(
  page: Instance,
  opener: InstalledExtension,
): Promise<{ action: string; mail: Record<string, unknown> } | null> {
  const text = await inFrame<string>(
    page,
    opener,
    `return document.getElementById('request')?.textContent ?? ''`,
  )
  return text ? JSON.parse(text) : null
}

scenario('contacts-mail-openers', { timeoutMs: 240_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop'] } })
  const page = group.device('anna/laptop').page as Instance

  const anna = await createContact(page, {
    head: { givenName: 'Anna', familyName: 'Beck' },
    values: [{ kind: 'email', label: 'other', value: 'anna@example.org' }],
    groups: [],
  })
  await openContacts(page)
  await page.click(`contacts-row-${anna}`)
  await page.waitForDisplayed('contacts-open-mail')
  assert.equal(await mailButton(page), 'plain', 'no extension: no menu')
  ctx.step('plain button without an extension')

  const opener = await install(page, fixture('e2e', 'mail-opener'))
  await ctx.waitFor(
    'the mail button to ask',
    async () => (await mailButton(page)) === 'menu',
    { timeoutMs: 15_000 },
  )

  // The menu: the system and the extension. Closing it without a choice opens nothing.
  await page.click('contacts-open-mail')
  await page.waitForDisplayed('contacts-open-system')
  await page.waitForDisplayed(`contacts-open-with-${opener.id}`)
  await page.exec(
    `document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`,
  )
  await ctx.waitFor(
    'the menu to close',
    async () =>
      !(await page.exec<boolean>(
        `return !!document.querySelector('[data-testid="contacts-open-system"]')`,
      )),
    { timeoutMs: 5_000 },
  )
  assert.equal(
    await page.exec<boolean>(`return !!document.querySelector(arguments[0])`, [
      frameOf(opener.id),
    ]),
    false,
    'nothing opened without a choice',
  )
  ctx.step('menu shown and closed without a choice')

  // The choice opens the extension, which takes the fields (not the link).
  await page.click('contacts-open-mail')
  await page.click(`contacts-open-with-${opener.id}`)
  await page.waitForDisplayed(frameOf(opener.id), 20_000)
  await ctx.waitFor(
    'the extension to show the fields',
    async () => (await pageRequest(page, opener)) !== null,
    { timeoutMs: 20_000 },
  )
  assert.deepEqual(await pageRequest(page, opener), {
    action: 'mail.compose',
    mail: {
      to: ['anna@example.org'],
      cc: [],
      bcc: [],
      subject: '',
      body: '',
    },
  })
  ctx.step('the extension took the fields')

  // Switched off, the extension is not offered any more and the button is the plain one again.
  unwrap(
    'extension_set_enabled',
    await page.invoke('extension_set_enabled', {
      extensionId: opener.id,
      enabled: false,
    }),
  )
  // Its tab closes with it; the address book comes back to the front.
  await openContacts(page)
  await ctx.waitFor(
    'the mail button to be plain again',
    async () => (await mailButton(page)) === 'plain',
    { timeoutMs: 15_000 },
  )
  ctx.step('plain again after switching the extension off')
})
