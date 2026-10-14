import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { unwrap } from '../lib/flows.ts'
import { textOf, typeChecked } from '../lib/fields.ts'
import { choose, openSettings, waitForLocation } from '../lib/settings.ts'
import { listFolder, startWebdav } from '../lib/webdav.ts'

const FOLDER = 'Fotos'

type Overview = {
  connections: Array<{
    id: string
    protocol: string
    endpoint: string
    credentials: string
    endpointScope: string
  }>
  storages: Array<{ id: string; name: string; bucket: string }>
}

type Header = {
  id: string
  title: string | null
  owner?: string | null
  tags: Array<{ name: string }>
}

// Spec 050, US1 (quickstart §5, SC-001, SC-007): in Einstellungen → Cloud-Speicher the user
// connects a local WebDAV server. A wrong password is named and saves nothing; the folder of the
// storage is missing, which holzi names and makes only when the user says so (FR-006); the
// credentials land in the password manager as an entry of `storage`; no test file stays on the
// server. After the sync the storage is on the second device and passes its test there. Removing
// the connection takes its storage and credentials with it. Needs Docker; skipped without it.
scenario(
  'storage-webdav',
  { timeoutMs: 360_000, needs: { container: true } },
  async (ctx) => {
    const webdav = await startWebdav()
    ctx.onTeardown(() => webdav.stop())
    ctx.step('WebDAV server started')

    const group = await ctx.group({ users: { anna: ['laptop', 'phone'] } })
    const laptop = group.device('anna/laptop').page
    const phone = group.device('anna/phone').page
    const overview = async (device: typeof laptop) =>
      unwrap<Overview>('storage_list', await device.invoke('storage_list'))

    await openSettings(laptop)
    await laptop.click('settings-category-storage')
    await waitForLocation(laptop, 'storage')
    const opened = Date.now()
    await laptop.click('storage-add-connection')
    await waitForLocation(laptop, 'storage.connection')
    await choose(laptop, 'storage-protocol', 'webdav')
    await laptop.waitForDisplayed('storage-username', 20_000)
    await choose(laptop, 'storage-provider-kind', 'other')
    await typeChecked(laptop, 'storage-endpoint', webdav.endpoint)
    await laptop.waitForDisplayed('storage-endpoint-insecure', 20_000)
    await typeChecked(laptop, 'storage-username', webdav.username)
    await typeChecked(laptop, 'storage-password', 'not-the-password')
    await typeChecked(laptop, 'storage-bucket', FOLDER)
    await typeChecked(laptop, 'storage-name', 'Fotos')
    await laptop.click('storage-save')
    await laptop.waitForDisplayed('storage-failure', 20_000)
    assert.match(
      await textOf(laptop, 'storage-failure'),
      /Zugangsdaten falsch|Credentials wrong/,
    )
    assert.deepEqual((await overview(laptop)).connections, [], 'nothing saved')
    ctx.step('a wrong password is named and saves nothing')

    await typeChecked(laptop, 'storage-password', webdav.password)
    await laptop.click('storage-save')
    await laptop
      .waitForDisplayed('storage-create-folder', 20_000)
      .catch(async (error: Error) => {
        const shown = await textOf(laptop, 'storage-failure').catch(() => '')
        throw new Error(
          `${error.message}; the form says: ${shown || 'nothing'}`,
        )
      })
    assert.match(
      await textOf(laptop, 'storage-failure'),
      /Ordner fehlt|Folder missing/,
    )
    assert.ok(
      !(await listFolder(webdav, '/')).includes(FOLDER),
      'holzi makes no folder on its own (FR-006)',
    )
    assert.deepEqual((await overview(laptop)).connections, [])
    ctx.step('the missing folder is named, nothing is made or saved')

    const saving = Date.now()
    await laptop.click('storage-create-folder')
    await waitForLocation(laptop, 'storage', 20_000)
    await laptop.waitForDisplayed('storage-row', 20_000)
    await laptop.waitForDisplayed('storage-insecure', 20_000)
    ctx.step(
      'folder made and connected after a passed test',
      `form ${Math.round((Date.now() - opened) / 1000)} s, test and save ${Date.now() - saving} ms`,
    )

    const saved = await overview(laptop)
    assert.equal(saved.connections.length, 1)
    assert.equal(saved.connections[0].protocol, 'webdav')
    assert.equal(saved.connections[0].endpointScope, 'local')
    assert.deepEqual(
      saved.storages.map((s) => [s.name, s.bucket]),
      [['Fotos', FOLDER]],
    )
    assert.ok((await listFolder(webdav, '/')).includes(FOLDER))
    assert.deepEqual(
      await listFolder(webdav, `/${FOLDER}/`),
      [],
      'no test file and no test folder stay on the server',
    )
    const headers = unwrap<{ headers: Header[] }>(
      'passwords_load_overview',
      await laptop.invoke('passwords_load_overview'),
    ).headers
    assert.equal(headers.length, 1)
    assert.match(headers[0]!.title ?? '', /^WebDAV: /)
    assert.equal(headers[0]!.owner, 'storage')
    assert.deepEqual(
      headers[0]!.tags.map((tag) => tag.name),
      ['haex-cloud-storage'],
      'spec 052 FR-005: findable by the tag',
    )
    ctx.step('credentials in the password manager, nothing left on the server')

    const storageId = saved.storages[0]!.id
    await ctx.waitFor(
      'the storage and its credentials to reach the phone',
      async () => {
        const there = await overview(phone)
        return (
          there.storages.some((s) => s.id === storageId) &&
          there.connections[0]?.credentials === 'present'
        )
      },
      { timeoutMs: 60_000, fixed: true },
    )
    const tested = unwrap<{ outcome: string; leftoverKey: string | null }>(
      'storage_test',
      await phone.invoke('storage_test', { id: storageId }),
    )
    assert.deepEqual(tested, { outcome: 'passed', leftoverKey: null })
    ctx.step('the phone has the storage and tests it with the synced login')

    await laptop.click('storage-connection-row')
    await waitForLocation(laptop, 'storage.connection')
    await laptop.click('storage-remove-connection')
    await waitForLocation(laptop, 'storage.remove')
    await laptop.click('storage-removal-confirm')
    await waitForLocation(laptop, 'storage')
    assert.deepEqual(await overview(laptop), { connections: [], storages: [] })
    assert.ok(
      (await listFolder(webdav, '/')).includes(FOLDER),
      'the folder stays at the provider',
    )
    ctx.step('removed; the folder stays on the server')
  },
)
