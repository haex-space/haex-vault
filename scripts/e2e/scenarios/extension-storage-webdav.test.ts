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
import { KEY } from '../lib/settings.ts'
import { listTree, startWebdav } from '../lib/webdav.ts'
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

const textOf = (page: Page, hook: string) =>
  page.exec<string>(
    `return document.querySelector('[data-testid="' + arguments[0] + '"]').textContent.trim()`,
    [hook],
  )

async function replaceText(page: Page, hook: string, text: string) {
  await page.type(hook, `${KEY.control}a${KEY.release}${KEY.backspace}`)
  await page.type(hook, text)
}

// Spec 050, US3 and US6 (quickstart §6): an extension proposes a WebDAV storage. The add
// permission names host and port of the address; holzi's dialog over the tab marks the local,
// unencrypted address and has no field for a login; holzi's window over the whole app asks for
// username and password, names a wrong one, says the folder is missing and makes it only when the
// user says so. Then the extension uses the storage as it would an S3 one: objects in its own
// area, a name that differs only in case refused, and the folders it leaves empty removed up to
// its area. Needs Docker; skipped without it.
scenario(
  'extension-storage-webdav',
  { timeoutMs: 300_000, needs: { container: true } },
  async (ctx) => {
    const webdav = await startWebdav()
    ctx.onTeardown(() => webdav.stop())
    ctx.step('WebDAV server started')
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

    const proposal = {
      request: {
        name: 'Ablage',
        type: 'webdav',
        config: { url: webdav.endpoint, folder: FOLDER },
      },
    }
    const asked = await probeRequest(
      page,
      probe,
      'extension_remote_storage_add_backend',
      proposal,
    )
    assert.equal(asked.error?.code, 1004)
    assert.deepEqual(asked.error?.details, {
      resourceType: 'remoteStorage',
      action: 'add',
      target: new URL(webdav.endpoint).host,
    })
    await page.waitForDisplayed('extension-permission-request', 10_000)
    await page.click('extension-permission-allow')
    ctx.step('the add permission names host and port of the address')

    await begin(page, probe, 'extension_remote_storage_add_backend', proposal)
    await page.waitForDisplayed('storage-dialog', 10_000)
    await page.waitForDisplayed('storage-dialog-local')
    await page.waitForDisplayed('storage-dialog-insecure')
    await page.waitForDisplayed('storage-dialog-create-folder')
    assert.equal(
      await page.exec<number>(
        `return [...document.querySelectorAll('[data-testid="storage-dialog"] input')]
           .filter((el) => el.type !== 'radio' && el.type !== 'checkbox').length`,
      ),
      0,
      'no field for a login over the tab',
    )
    await page.click('storage-dialog-confirm')
    await page.waitForDisplayed('storage-credentials-modal', 10_000)
    await page.type('storage-credentials-username', webdav.username)
    await page.type('storage-credentials-password', 'not-the-password')
    await page.type('storage-credentials-password', KEY.enter)
    await page.waitForDisplayed('storage-credentials-failure', 20_000)
    assert.match(
      await textOf(page, 'storage-credentials-failure'),
      /Zugangsdaten falsch|Credentials wrong/,
    )
    await replaceText(page, 'storage-credentials-password', webdav.password)
    await page.type('storage-credentials-password', KEY.enter)
    await page.waitForDisplayed('storage-credentials-create-folder', 20_000)
    assert.match(
      await textOf(page, 'storage-credentials-failure'),
      /Ordner fehlt|Folder missing/,
    )
    assert.ok(
      !(await listTree(webdav, '/')).includes(`${FOLDER}/`),
      'no folder without the user’s word (FR-006)',
    )
    await page.click('storage-credentials-create-folder')
    const added = await answer(page, probe)
    assert.equal(added.error, undefined, JSON.stringify(added))
    const storage = added.result as { id: string; type: string; folder: string }
    assert.equal(storage.type, 'webdav')
    assert.equal(storage.folder, FOLDER)
    assert.ok(
      !JSON.stringify(added).includes(webdav.password) &&
        !JSON.stringify(added).includes(webdav.username),
      'the extension never gets the login',
    )
    ctx.step(
      'a login typed in holzi’s window; the folder made on the user’s word',
    )

    const listed = await probeRequest(
      page,
      probe,
      'extension_remote_storage_list_backends',
      {},
    )
    assert.deepEqual(listed.result, [
      {
        id: storage.id,
        type: 'webdav',
        name: 'Ablage',
        providerName: '127.0.0.1',
        folder: FOLDER,
      },
    ])

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
    const onServer = await listTree(webdav, `/${FOLDER}/`)
    const file = onServer.find((path) => path.endsWith('/a/b.txt')) ?? ''
    assert.match(file, /^holzi-ext\/[0-9a-f-]{36}\/[0-9a-f-]{36}\/a\/b\.txt$/)
    const area = file.slice(0, -'a/b.txt'.length)
    const got = await probeRequest(
      page,
      probe,
      'extension_remote_storage_download',
      { request: object },
    )
    assert.equal(Buffer.from(String(got.result), 'base64').toString(), 'hallo')
    const inArea = await probeRequest(
      page,
      probe,
      'extension_remote_storage_list',
      { request: { backendId: storage.id, prefix: 'a/b' } },
    )
    assert.deepEqual(
      (inArea.result as { key: string }[]).map((o) => o.key),
      ['a/b.txt'],
      'no folders, a prefix may end inside a name',
    )
    const clash = await probeRequest(
      page,
      probe,
      'extension_remote_storage_upload',
      {
        request: {
          backendId: storage.id,
          key: 'a/B.txt',
          data: Buffer.from('x').toString('base64'),
        },
      },
    )
    assert.equal(clash.error?.code, 3001)
    await probeRequest(page, probe, 'extension_remote_storage_delete', {
      request: object,
    })
    const left = await listTree(webdav, `/${FOLDER}/`)
    assert.ok(left.includes(area), `the area stays: ${left.join(', ')}`)
    assert.ok(
      !left.some((path) => path.startsWith(`${area}a`)),
      `the emptied folder a/ is gone: ${left.join(', ')}`,
    )
    assert.ok(!left.some((path) => path.includes('.holzi-part-')))
    ctx.step('objects in the extension’s own area, round trip against WebDAV')
  },
)
