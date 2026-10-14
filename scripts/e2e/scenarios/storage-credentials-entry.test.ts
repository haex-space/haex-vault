import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { unwrap } from '../lib/flows.ts'
import { textOf, typeChecked } from '../lib/fields.ts'
import {
  createEntry,
  overview as passwords,
  trashEntry,
  updateEntry,
} from '../lib/passwords.ts'
import {
  choose,
  isShown,
  openSettings,
  waitForLocation,
} from '../lib/settings.ts'
import { startWebdav } from '../lib/webdav.ts'

const FOLDER = 'Ablage'

type Overview = {
  connections: Array<{
    id: string
    protocol: string
    credentials: string
    credentialsEntry?: { id: string; title: string | null; owned: boolean }
  }>
  storages: Array<{ id: string; bucket: string }>
}

// Spec 052, US1 (quickstart §2 steps 1–2, SC-002): the login of a local WebDAV server is already an
// entry of the password manager. In Einstellungen → Cloud-Speicher the user chooses it instead of
// typing; only entries that fit the protocol are offered. holzi tests with its values, saves the
// connection pointing at it, and the password manager holds no second entry. US2 (quickstart §2
// step 5): the chosen entry carries the tag of cloud storage credentials, and the form names it
// and opens it in the password manager. US3 (§2 steps 3–4): a changed password applies on the
// next call; deleting the entry warns first, then the connection asks for new credentials and
// runs again with another entry. Needs Docker; skipped without it.
scenario(
  'storage-credentials-entry',
  { timeoutMs: 300_000, needs: { container: true } },
  async (ctx) => {
    const webdav = await startWebdav()
    ctx.onTeardown(() => webdav.stop())
    ctx.step('WebDAV server started')

    const group = await ctx.group({ users: { anna: ['laptop'] } })
    const laptop = group.device('anna/laptop').page
    const login = await createEntry(laptop, {
      title: 'Mein WebDAV',
      username: webdav.username,
      password: webdav.password,
      url: webdav.endpoint,
    })
    const nameOnly = await createEntry(laptop, {
      title: 'Nur ein Name',
      username: 'anna',
    })
    ctx.step('two entries in the password manager, one with a password')

    await openSettings(laptop)
    await laptop.click('settings-category-storage')
    await waitForLocation(laptop, 'storage')
    await laptop.click('storage-add-connection')
    await waitForLocation(laptop, 'storage.connection')
    await choose(laptop, 'storage-protocol', 'webdav')
    await laptop.waitForDisplayed('storage-username', 20_000)
    await choose(laptop, 'storage-provider-kind', 'other')
    await typeChecked(laptop, 'storage-endpoint', webdav.endpoint)
    await choose(laptop, 'storage-credentials-source', 'entry')
    await laptop.waitForDisplayed(`storage-entry-option-${login}`, 20_000)
    assert.equal(
      await isShown(laptop, `[data-testid="storage-entry-option-${nameOnly}"]`),
      false,
      'an entry without a password does not fit WebDAV (FR-002)',
    )
    ctx.step('only the fitting entry is offered')

    await laptop.click(`storage-entry-option-${login}`)
    await typeChecked(laptop, 'storage-bucket', FOLDER)
    await laptop.click('storage-save')
    await laptop
      .waitForDisplayed('storage-create-folder', 20_000)
      .catch(async (error: Error) => {
        const shown = await textOf(laptop, 'storage-failure').catch(() => '')
        throw new Error(
          `${error.message}; the form says: ${shown || 'nothing'}`,
        )
      })
    await laptop.click('storage-create-folder')
    await waitForLocation(laptop, 'storage', 20_000)
    await laptop.waitForDisplayed('storage-row', 20_000)
    ctx.step('tested with the entry and saved')

    const saved = unwrap<Overview>(
      'storage_list',
      await laptop.invoke('storage_list'),
    )
    assert.equal(saved.connections.length, 1)
    assert.equal(saved.connections[0]!.protocol, 'webdav')
    assert.equal(saved.connections[0]!.credentials, 'present')
    assert.deepEqual(
      saved.storages.map((s) => s.bucket),
      [FOLDER],
    )
    const entries = (await passwords(laptop)).headers.map((h) => h.id).sort()
    assert.deepEqual(
      entries,
      [login, nameOnly].sort(),
      'SC-002: no second entry for the connection',
    )
    ctx.step('the connection points at the chosen entry, no copy')

    const chosen = (await passwords(laptop)).headers.find((h) => h.id === login)
    assert.ok(
      chosen?.tags.some((tag) => tag.name === 'haex-cloud-storage'),
      'FR-006: the chosen entry carries the tag',
    )
    assert.deepEqual(saved.connections[0]!.credentialsEntry, {
      id: login,
      title: 'Mein WebDAV',
      owned: false,
    })
    ctx.step('the chosen entry carries the tag')

    // US3 (quickstart §2 steps 3–4): the entry is the only source of the values.
    const storageId = saved.storages[0]!.id
    const test = async () =>
      unwrap<{ outcome: string }>(
        'storage_test',
        await laptop.invoke('storage_test', { id: storageId }),
      ).outcome
    await updateEntry(laptop, login, { password: 'not-the-password' })
    assert.equal(await test(), 'accessDenied', 'the next call uses the change')
    await updateEntry(laptop, login, { password: webdav.password })
    assert.equal(await test(), 'passed')
    ctx.step('a changed value in the entry applies on the next call')

    const usage = unwrap<{ features: string[] }>(
      'passwords_item_usage',
      await laptop.invoke('passwords_item_usage', { args: { itemId: login } }),
    )
    assert.deepEqual(
      usage.features,
      ['storage'],
      'the password manager warns before deleting it (spec 034 FR-034)',
    )
    const other = await createEntry(laptop, {
      title: 'Zweites WebDAV',
      username: webdav.username,
      password: webdav.password,
    })
    await trashEntry(laptop, login)
    await laptop.waitForDisplayed('storage-credentials-missing', 20_000)
    await laptop.click('storage-connection-row')
    await waitForLocation(laptop, 'storage.connection')
    await laptop.waitForDisplayed('storage-credentials-missing-note', 20_000)
    ctx.step('a deleted entry asks for new credentials')

    await laptop.click('storage-credentials-choose-other')
    await laptop.click(`storage-entry-option-${other}`)
    await laptop.click('storage-save')
    await waitForLocation(laptop, 'storage', 20_000)
    const relinked = unwrap<Overview>(
      'storage_list',
      await laptop.invoke('storage_list'),
    ).connections[0]!
    assert.equal(relinked.credentials, 'present')
    assert.equal(relinked.credentialsEntry?.id, other)
    assert.equal(await test(), 'passed')
    ctx.step('another entry chosen, the connection runs again')

    await laptop.click('storage-connection-row')
    await waitForLocation(laptop, 'storage.connection')
    await laptop.click('storage-open-credentials-entry')
    await laptop.waitForDisplayed('passwords-edit', 20_000)
    await ctx.waitFor(
      'the password manager to show the chosen entry',
      async () =>
        (await textOf(laptop, 'passwords-entry-title').catch(() => '')) ===
        'Zweites WebDAV',
      { fixed: true, timeoutMs: 10_000 },
    )
    ctx.step('the form names the entry and opens it in the password manager')
  },
)
