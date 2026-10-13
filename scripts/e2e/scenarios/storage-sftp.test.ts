import assert from 'node:assert/strict'
import { textOf, typeChecked } from '../lib/fields.ts'
import { unwrap } from '../lib/flows.ts'
import { scenario } from '../lib/scenario.ts'
import { choose, openSettings, waitForLocation } from '../lib/settings.ts'
import { prepareSftp } from '../lib/sftp.ts'

const FOLDER = 'Ablage'

type Overview = {
  connections: Array<{
    id: string
    protocol: string
    credentials: string
    hostKey?: { fingerprint: string }
  }>
  storages: Array<{ id: string; name: string; bucket: string }>
}

type Header = { id: string; title: string | null; owner?: string | null }

// Spec 050, US4 (quickstart §3, SC-004, FR-008, FR-012): in Einstellungen → Cloud-Speicher the
// user connects a local SFTP server with a key holzi generates; only its public key reaches the
// window, and it is registered with the server. The first test stops at the unknown host key and
// shows its fingerprint, which is compared with the server's; after the confirmation the
// connection is saved with the key in the password manager. The server then shows another host
// key: the storage's test names the old and the new fingerprint, and after the user takes the new
// one the test passes, also on the second device with the synced key. Needs Docker; skipped
// without it.
scenario(
  'storage-sftp',
  { timeoutMs: 420_000, needs: { container: true } },
  async (ctx) => {
    const sftp = await prepareSftp()
    ctx.onTeardown(() => sftp.stop())
    sftp.makeFolder(FOLDER)

    const group = await ctx.group({ users: { anna: ['laptop', 'phone'] } })
    const laptop = group.device('anna/laptop').page
    const phone = group.device('anna/phone').page
    const overview = async (device: typeof laptop) =>
      unwrap<Overview>('storage_list', await device.invoke('storage_list'))

    await openSettings(laptop)
    await laptop.click('settings-category-storage')
    await waitForLocation(laptop, 'storage')
    await laptop.click('storage-add-connection')
    await waitForLocation(laptop, 'storage.connection')
    await choose(laptop, 'storage-protocol', 'sftp')
    await laptop.waitForDisplayed('storage-username')
    await choose(laptop, 'storage-provider-kind', 'other')
    await typeChecked(laptop, 'storage-endpoint', sftp.endpoint)
    await typeChecked(laptop, 'storage-username', sftp.username)
    await choose(laptop, 'storage-auth-method', 'key')
    await laptop.click('storage-generate-key')
    await laptop.waitForDisplayed('storage-public-key', 10_000)
    const publicKey = await textOf(laptop, 'storage-public-key')
    assert.match(publicKey, /^ssh-ed25519 \S+ holzi$/)
    assert.ok(
      !(await laptop.exec<string>(`return document.body.innerText`)).includes(
        'PRIVATE KEY',
      ),
      'the private key never reaches the window (FR-008)',
    )
    sftp.authorize(publicKey)
    await sftp.startAsync()
    ctx.step('a key generated in holzi, its public key at the server')

    await typeChecked(laptop, 'storage-bucket', FOLDER)
    await typeChecked(laptop, 'storage-name', 'Ablage')
    await laptop.click('storage-save')
    await laptop.waitForDisplayed('storage-host-key', 30_000)
    assert.equal(
      await textOf(laptop, 'storage-host-key-fingerprint'),
      sftp.fingerprint,
      'the fingerprint the server has',
    )
    assert.deepEqual((await overview(laptop)).connections, [], 'nothing saved')
    assert.deepEqual(
      sftp.listFolder(FOLDER),
      [],
      'nothing reached the folder before the key was confirmed',
    )
    ctx.step('the unknown host key is shown and nothing is saved')

    await laptop.click('storage-host-key-confirm')
    await waitForLocation(laptop, 'storage', 30_000)
    await laptop.waitForDisplayed('storage-row')
    const saved = await overview(laptop)
    assert.equal(saved.connections[0]?.protocol, 'sftp')
    assert.equal(saved.connections[0]?.hostKey?.fingerprint, sftp.fingerprint)
    assert.deepEqual(sftp.listFolder(FOLDER), [], 'no test file stays')
    const headers = unwrap<{ headers: Header[] }>(
      'passwords_load_overview',
      await laptop.invoke('passwords_load_overview'),
    ).headers
    assert.match(headers[0]?.title ?? '', /^SFTP: /)
    assert.equal(headers[0]?.owner, 'storage')
    ctx.step('confirmed and saved; the key is in the password manager')

    const before = sftp.fingerprint
    await sftp.startAsync({ newHostKey: true })
    assert.notEqual(sftp.fingerprint, before)
    await laptop.click('storage-row')
    await waitForLocation(laptop, 'storage.storage')
    await laptop.click('storage-test')
    await laptop.waitForDisplayed('storage-host-key', 30_000)
    assert.equal(await textOf(laptop, 'storage-host-key-previous'), before)
    assert.equal(
      await textOf(laptop, 'storage-host-key-fingerprint'),
      sftp.fingerprint,
    )
    await laptop.click('storage-host-key-confirm')
    await laptop.waitForDisplayed('storage-test-passed', 30_000)
    ctx.step('a changed host key is named with both fingerprints and taken')

    const storageId = saved.storages[0]!.id
    await ctx.waitFor(
      'the connection with the new key to reach the phone',
      async () => {
        const there = await overview(phone)
        return (
          there.storages.some((s) => s.id === storageId) &&
          there.connections[0]?.credentials === 'present' &&
          there.connections[0]?.hostKey?.fingerprint === sftp.fingerprint
        )
      },
      { timeoutMs: 60_000, fixed: true },
    )
    const tested = unwrap<{ outcome: string }>(
      'storage_test',
      await phone.invoke('storage_test', { id: storageId }),
    )
    assert.equal(tested.outcome, 'passed')
    ctx.step('the phone tests the storage with the synced key and host key')
  },
)
