import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import {
  fixture,
  inFrame,
  install,
  openFromLauncher,
  probeRequest,
  type BridgeAnswer,
  type InstalledExtension,
} from '../lib/extensions.ts'
import { textOf } from '../lib/fields.ts'
import { KEY } from '../lib/settings.ts'
import { prepareSftp } from '../lib/sftp.ts'
import type { Page } from '../lib/page.ts'

const FOLDER = 'Erweiterung'

/** Starts a bridge request in the probe without waiting; `answer` reads its answer later. */
async function begin(
  page: Page,
  probe: InstalledExtension,
  method: string,
  params: unknown,
): Promise<void> {
  await inFrame(
    page,
    probe,
    `window.answer = window.probe.request(arguments[0], arguments[1])
       .then((a) => JSON.parse(JSON.stringify(a)))`,
    [method, params],
  )
}

function answer(page: Page, probe: InstalledExtension): Promise<BridgeAnswer> {
  return inFrame<BridgeAnswer>(page, probe, `return window.answer`)
}

// Spec 050, US5 and US6 (SFTP): an extension proposes an SFTP storage by host, port and folder.
// The add permission names `host:port`; holzi's window over the whole app asks for the login,
// then shows the host key the server presents, and saves only after the user confirmed it; the
// dialog over the tab never shows a key. Then the extension uses the storage as an S3 one: objects
// in its own area, a round trip, and the folders it leaves empty removed. Needs Docker; skipped
// without it.
scenario(
  'extension-storage-sftp',
  { timeoutMs: 300_000, needs: { container: true } },
  async (ctx) => {
    const sftp = await prepareSftp({ password: 'sftp-e2e-password' })
    ctx.onTeardown(() => sftp.stop())
    sftp.makeFolder(FOLDER)
    await sftp.startAsync()
    ctx.step('SFTP server started')
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

    const where = new URL(sftp.endpoint.replace('sftp://', 'http://'))
    const proposal = {
      request: {
        name: 'Ablage',
        type: 'sftp',
        config: {
          host: where.hostname,
          port: Number(where.port),
          folder: FOLDER,
        },
      },
    }
    const asked = await probeRequest(
      page,
      probe,
      'extension_remote_storage_add_backend',
      proposal,
    )
    assert.equal(asked.error?.code, 1004)
    assert.equal(
      (asked.error?.details as { target?: string } | undefined)?.target,
      where.host,
    )
    await page.waitForDisplayed('extension-permission-request', 10_000)
    await page.click('extension-permission-allow')
    ctx.step('the add permission names host and port')

    await begin(page, probe, 'extension_remote_storage_add_backend', proposal)
    await page.waitForDisplayed('storage-dialog', 10_000)
    await page.waitForDisplayed('storage-dialog-local')
    await page.click('storage-dialog-confirm')
    await page.waitForDisplayed('storage-credentials-modal', 10_000)
    await page.type('storage-username', sftp.username)
    await page.type('storage-password', sftp.password ?? '')
    await page.type('storage-password', KEY.enter)
    await page.waitForDisplayed('storage-host-key', 20_000)
    assert.equal(
      await textOf(page, 'storage-host-key-fingerprint'),
      sftp.fingerprint,
      'the fingerprint the server has',
    )
    assert.deepEqual(sftp.listFolder(FOLDER), [], 'nothing written yet')
    await page.click('storage-host-key-confirm')
    const added = await answer(page, probe)
    assert.equal(added.error, undefined, JSON.stringify(added))
    const storage = added.result as { id: string; type: string; folder: string }
    assert.equal(storage.type, 'sftp')
    assert.equal(storage.folder, FOLDER)
    assert.ok(
      !JSON.stringify(added).includes(sftp.password ?? '-') &&
        !JSON.stringify(added).includes(sftp.hostKey.split(' ')[1] ?? '-'),
      'the extension gets neither the login nor the host key',
    )
    ctx.step('the host key confirmed in holzi’s window, the storage added')

    const object = { backendId: storage.id, key: 'a/b.txt' }
    const put = await probeRequest(
      page,
      probe,
      'extension_remote_storage_upload',
      {
        request: { ...object, data: Buffer.from('hallo').toString('base64') },
      },
    )
    assert.equal(put.error, undefined, JSON.stringify(put))
    const got = await probeRequest(
      page,
      probe,
      'extension_remote_storage_download',
      { request: object },
    )
    assert.equal(Buffer.from(String(got.result), 'base64').toString(), 'hallo')
    const listed = await probeRequest(
      page,
      probe,
      'extension_remote_storage_list',
      { request: { backendId: storage.id } },
    )
    assert.deepEqual(
      (listed.result as { key: string }[]).map((o) => o.key),
      ['a/b.txt'],
    )
    await probeRequest(page, probe, 'extension_remote_storage_delete', {
      request: object,
    })
    const left = sftp.listTree(FOLDER)
    assert.ok(
      !left.some((path) => path.endsWith('/a/')),
      `the emptied folder a/ is gone: ${left.join(', ')}`,
    )
    assert.ok(!left.some((path) => path.includes('.holzi-')))
    ctx.step('objects in the extension’s own area, round trip against SFTP')
  },
)
