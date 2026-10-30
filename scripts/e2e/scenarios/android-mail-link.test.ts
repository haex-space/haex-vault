import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import {
  fixture,
  frameOf,
  inFrame,
  install,
  type InstalledExtension,
} from '../lib/extensions.ts'
import type { Page } from '../lib/page.ts'

// Spec 063 US2 (FR-017 to FR-020): a `mailto:` link opened outside holzi, as another app does,
// shows the choice, with a row for the extension that writes mail and never one for the system's
// mail program. A tap opens the extension with the fields of the link. With the vault locked the
// link waits and the choice follows the unlocking; a second link replaces the first; a link with a
// line break in the subject is refused and says so. (Links of the `SENDTO` action are tried by hand
// in the quickstart: the E2E phone control only sends `VIEW`.)

const BEN = 'mailto:ben@example.org?subject=Hallo&body=Zeile%201%0D%0AZeile%202'
const ADA = 'mailto:ada@example.org?subject=Zweite'
// Not BEN again: the same link within three seconds counts as the one a start brings twice
// (`REPEAT_WITHIN_MS`), and the steps before take less than that on a fast runner.
const CARA = 'mailto:cara@example.org?subject=Erste'
const BROKEN = 'mailto:ben@example.org?subject=a%0d%0aBcc:x@example.org'

const text = (page: Page, testid: string) =>
  page.exec<string>(
    `return document.querySelector('[data-testid="' + arguments[0] + '"]')?.textContent ?? ''`,
    [testid],
  )

const shown = (page: Page, testid: string) =>
  page.exec<number>(
    `return document.querySelectorAll('[data-testid="' + arguments[0] + '"]').length`,
    [testid],
  )

async function pageRequest(
  page: Page,
  opener: InstalledExtension,
): Promise<{ mail: Record<string, unknown> } | null> {
  const raw = await inFrame<string>(
    page,
    opener,
    `return document.getElementById('request')?.textContent ?? ''`,
  )
  return raw ? JSON.parse(raw) : null
}

scenario(
  'android-mail-link',
  { needs: { phone: true }, timeoutMs: 360_000 },
  async (ctx) => {
    const { passphrase } = ctx.credentials()
    const first = await ctx.startInstance()
    await createAndUnlock(first, { name: 'phone-vault', passphrase })
    const opener = await install(first, fixture('e2e', 'mail-opener'))

    // With the vault open: the choice has the extension and no row for the system.
    first.phone!.openLink(BEN)
    await first.waitForDisplayed('mail-link-dialog', 30_000)
    await first.waitForDisplayed(`mail-link-open-${opener.id}`)
    assert.match(await text(first, 'mail-link-dialog'), /ben@example\.org/)
    assert.equal(await shown(first, 'contacts-open-system'), 0)
    assert.equal(await shown(first, 'mail-link-error'), 0)
    ctx.step('the choice shows the extension and not the system')

    // The tap opens the extension, which takes the fields (not the link).
    await first.click(`mail-link-open-${opener.id}`)
    await first.waitForDisplayed(frameOf(opener.id), 20_000)
    await ctx.waitFor(
      'the extension to show the fields',
      async () => (await pageRequest(first, opener)) !== null,
      { timeoutMs: 20_000 },
    )
    assert.deepEqual((await pageRequest(first, opener))?.mail, {
      to: ['ben@example.org'],
      cc: [],
      bcc: [],
      subject: 'Hallo',
      body: 'Zeile 1\nZeile 2',
    })
    ctx.step('the extension took the fields')

    // Two links in a row: the choice shows the second.
    first.phone!.openLink(CARA)
    await first.waitForDisplayed('mail-link-dialog', 30_000)
    first.phone!.openLink(ADA)
    await ctx.waitFor(
      'the choice to show the second link',
      async () =>
        /ada@example\.org/.test(await text(first, 'mail-link-dialog')),
      { timeoutMs: 20_000 },
    )
    assert.doesNotMatch(await text(first, 'mail-link-dialog'), /cara@/)
    ctx.step('a second link replaces the first')

    // A line break in the subject is refused and shown as unreadable, with no rows.
    first.phone!.openLink(BROKEN)
    await first.waitForDisplayed('mail-link-error', 30_000)
    assert.equal(await shown(first, `mail-link-open-${opener.id}`), 0)
    ctx.step('a link with a line break in the subject is refused')

    // With the vault locked the link waits and the choice follows the unlocking.
    first.phone!.killInBackground()
    await ctx.waitFor('the process to end', () => !first.alive(), {
      timeoutMs: 15_000,
      fixed: true,
    })
    await first.stop()
    const next = await ctx.startInstance({ reusesRoot: first.root })
    await next.waitForDisplayed('landing-import')
    next.phone!.openLink(BEN)
    await ctx.waitFor(
      'the link to wait in the inbox',
      async () =>
        (await next.exec<string>(
          `const pinia = document.querySelector('#__nuxt')?.__vue_app__?.config.globalProperties.$pinia
           return pinia?.state.value.mailInbox?.link ?? ''`,
        )) === BEN,
    )
    assert.equal(
      await shown(next, 'mail-link-dialog'),
      0,
      'no choice before unlocking',
    )

    await next.click('instance-entry')
    await next.type('unlock-passphrase', passphrase)
    await next.click('unlock-submit')
    await next.waitForDisplayed('mail-link-dialog', 60_000)
    await next.waitForDisplayed(`mail-link-open-${opener.id}`)
    assert.match(await text(next, 'mail-link-dialog'), /ben@example\.org/)
    ctx.step('after unlocking the choice follows')
  },
)
