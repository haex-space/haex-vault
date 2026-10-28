import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import { contactHeaders, createContact } from '../lib/contacts.ts'
import { shareLink } from '../lib/share.ts'

// Spec 060 US4 (FR-018a): a `haex-vault://contact/…` link opened outside holzi, as another app or the
// camera app does, opens the reading-in page with a preview and stores nothing. It works with the
// vault open, and with the vault locked: the link waits until the person unlocks and the preview
// follows. (The link of a cold start arrives after `setup` through the same event.)
scenario(
  'android-contacts-deeplink',
  { needs: { phone: true }, timeoutMs: 300_000 },
  async (ctx) => {
    const { passphrase } = ctx.credentials()
    const first = await ctx.startInstance()
    await createAndUnlock(first, { name: 'phone-vault', passphrase })
    const ben = await createContact(first, {
      head: { givenName: 'Ben', familyName: 'Krüger' },
      values: [{ kind: 'phone', label: 'mobile', value: '0170 1111' }],
      groups: [],
    })
    const link = await shareLink(first, ben)
    const known = (await contactHeaders(first)).length

    const previewName = (instance: typeof first) =>
      instance.exec<string>(
        `return document.querySelector('[data-testid="contacts-import-name"]')?.textContent ?? ''`,
      )

    // With the vault open: the system hands the link to the running app.
    first.phone!.openLink(link)
    await first.waitForDisplayed('contacts-import-preview', 30_000)
    assert.match(await previewName(first), /Ben Krüger/)
    assert.equal(
      (await contactHeaders(first)).length,
      known,
      'a preview stores nothing',
    )
    ctx.step(
      'with the vault open the link shows the preview and stores nothing',
    )

    // With the vault locked: the app starts again at the vault list and the link waits.
    first.phone!.killInBackground()
    await ctx.waitFor('the process to end', () => !first.alive(), {
      timeoutMs: 15_000,
      fixed: true,
    })
    await first.stop()
    const next = await ctx.startInstance({ reusesRoot: first.root })
    await next.waitForDisplayed('landing-import')
    next.phone!.openLink(link)
    await ctx.waitFor(
      'the link to wait in the inbox',
      async () =>
        (await next.exec<string>(
          `const pinia = document.querySelector('#__nuxt')?.__vue_app__?.config.globalProperties.$pinia
           return pinia?.state.value.shareInbox?.text ?? ''`,
        )) === link,
    )
    assert.equal(
      await next.exec<number>(
        `return document.querySelectorAll('[data-testid="contacts-import-preview"]').length`,
      ),
      0,
      'no preview before a vault is open',
    )
    ctx.step('with the vault locked the link waits and shows nothing')

    // The person unlocks in the same process: the preview comes up by itself.
    await next.click('instance-entry')
    await next.type('unlock-passphrase', passphrase)
    await next.click('unlock-submit')
    await next.waitForDisplayed('contacts-import-preview', 60_000)
    assert.match(await previewName(next), /Ben Krüger/)
    assert.equal(
      (await contactHeaders(next)).length,
      known,
      'still nothing stored',
    )
    ctx.step('after unlocking the preview follows, and nothing is stored yet')
  },
)
