import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { unwrap } from '../lib/flows.ts'
import {
  fixture,
  inFrame,
  install,
  openFromLauncher,
  probeRequest,
  type BridgeAnswer,
  type InstalledExtension,
} from '../lib/extensions.ts'
import { listKeys, startRustfs } from '../lib/rustfs.ts'
import { KEY } from '../lib/settings.ts'
import type { Page } from '../lib/page.ts'

const USERS_BUCKET = 'holzi-ext-e2e'
const PROPOSED_BUCKET = 'holzi-ext-two'

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

/** Text and password fields inside the element with `testid`. */
function textFields(page: Page, testid: string): Promise<number> {
  return page.exec<number>(
    `return [...document.querySelectorAll('[data-testid="' + arguments[0] + '"] input, [data-testid="' + arguments[0] + '"] textarea')]
       .filter((el) => el.type !== 'radio' && el.type !== 'checkbox').length`,
    [testid],
  )
}

// Spec 038, US2 and US3 (quickstart §5): an extension uses a storage the user made in the
// settings and proposes another bucket on the same provider. holzi confirms it over the tab with a
// dialog that has no field for credentials; new credentials are typed only in holzi's window over
// the whole app, which names wrong ones and stays open for a correction. Objects land in the
// extension's own area of the bucket. A call carrying credentials is refused without a dialog, and
// a local endpoint needs the add permission for its host first. Needs Docker for RustFS; skipped
// without it.
scenario(
  'extension-storage',
  { timeoutMs: 300_000, needs: { container: true } },
  async (ctx) => {
    const rustfs = await startRustfs([USERS_BUCKET, PROPOSED_BUCKET])
    ctx.onTeardown(() => rustfs.stop())
    ctx.step('RustFS started')
    const group = await ctx.group({ users: { anna: ['laptop'] } })
    const page = group.device('anna/laptop').page

    const credentials = {
      accessKeyId: rustfs.accessKeyId,
      secretAccessKey: rustfs.secretAccessKey,
    }
    const connection = unwrap<{ id: string }>(
      'storage_connection_save',
      await page.invoke('storage_connection_save', {
        input: {
          protocol: 's3',
          providerName: 'RustFS',
          providerKind: 'rustfs',
          endpoint: rustfs.endpoint,
          region: rustfs.region,
          addressing: 'path',
          credentials: { kind: 's3', ...credentials },
          bucketForTest: USERS_BUCKET,
        },
      }),
    )
    const storage = unwrap<{ id: string }>(
      'storage_save',
      await page.invoke('storage_save', {
        input: {
          connectionId: connection.id,
          name: 'Fotos',
          bucket: USERS_BUCKET,
        },
      }),
    )
    ctx.step('the user connected RustFS in the settings')

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
    unwrap(
      'extension_permission_set',
      await page.invoke('extension_permission_set', {
        args: {
          extensionId: probe.id,
          kind: 'remoteStorage',
          action: 'read',
          target: storage.id,
          status: 'granted',
          allDevices: true,
        },
      }),
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
        type: 's3',
        name: 'Fotos',
        providerName: 'RustFS',
        bucket: USERS_BUCKET,
      },
    ])
    ctx.step('the extension sees the storage by name only')

    await begin(page, probe, 'extension_remote_storage_add_backend', {
      request: {
        name: 'Zweiter',
        type: 's3',
        sameProviderAs: storage.id,
        config: { bucket: PROPOSED_BUCKET },
      },
    })
    await page.waitForDisplayed('storage-dialog', 10_000)
    assert.equal(
      await textFields(page, 'storage-dialog'),
      0,
      'no field for credentials over the tab',
    )
    await page.click('storage-dialog-confirm')
    const added = await answer(page, probe)
    assert.equal(added.error, undefined, JSON.stringify(added))
    const second = (added.result as { id: string }).id
    ctx.step('a bucket on the same provider, confirmed over the tab')

    const object = { backendId: second, key: 'a/b.txt' }
    const put = await probeRequest(
      page,
      probe,
      'extension_remote_storage_upload',
      {
        request: { ...object, data: Buffer.from('hallo').toString('base64') },
      },
    )
    assert.equal(put.error, undefined, JSON.stringify(put))
    const keys = await listKeys(rustfs, PROPOSED_BUCKET)
    assert.equal(keys.length, 1)
    assert.match(
      keys[0] ?? '',
      /^holzi-ext\/[0-9a-f-]{36}\/[0-9a-f-]{36}\/a\/b\.txt$/,
    )
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
      {
        request: { backendId: second },
      },
    )
    assert.deepEqual(
      (inArea.result as { key: string }[]).map((o) => o.key),
      ['a/b.txt'],
    )
    const escape = await probeRequest(
      page,
      probe,
      'extension_remote_storage_download',
      { request: { backendId: second, key: '../x' } },
    )
    assert.equal(escape.error?.code, 3001)
    await probeRequest(page, probe, 'extension_remote_storage_delete', {
      request: object,
    })
    assert.deepEqual(await listKeys(rustfs, PROPOSED_BUCKET), [])
    ctx.step('objects in the extension’s own area, round trip against RustFS')

    await begin(page, probe, 'extension_remote_storage_update_backend', {
      request: { backendId: second },
    })
    await page.waitForDisplayed('storage-dialog', 10_000)
    await page.click('storage-dialog-new-credentials')
    await page.click('storage-dialog-confirm')
    await page.waitForDisplayed('storage-credentials-modal', 10_000)
    await page.type('storage-access-key', rustfs.accessKeyId)
    await page.type('storage-secret', 'not-the-secret')
    // Enter in a field confirms, as the button does.
    await page.type('storage-secret', KEY.enter)
    await page.waitForDisplayed('storage-credentials-failure', 20_000)
    const refused = await page.exec<string>(
      `return document.querySelector('[data-testid="storage-credentials-failure"]').textContent.trim()`,
    )
    assert.match(refused, /Zugangsdaten falsch|Credentials wrong/)
    ctx.step('wrong credentials are named in holzi’s window, which stays open')

    await page.type(
      'storage-secret',
      `${KEY.control}a${KEY.release}${KEY.backspace}`,
    )
    await page.type('storage-secret', rustfs.secretAccessKey)
    await page.type('storage-secret', KEY.enter)
    const updated = await answer(page, probe)
    assert.equal(updated.error, undefined, JSON.stringify(updated))
    assert.ok(
      !JSON.stringify(updated).includes(rustfs.secretAccessKey),
      'the extension never gets the credentials',
    )
    ctx.step('corrected credentials typed in holzi’s window over the whole app')

    const withCredentials = await probeRequest(
      page,
      probe,
      'extension_remote_storage_add_backend',
      {
        request: {
          name: 'X',
          type: 's3',
          config: { region: 'eu', bucket: 'nope', ...credentials },
        },
      },
    )
    assert.equal(withCredentials.error?.code, 3001)
    assert.ok(
      !(await page.exec<boolean>(
        `return !!document.querySelector('[data-testid="storage-dialog"]')`,
      )),
      'no dialog for a call with credentials',
    )
    ctx.step('credentials in a call are refused without a dialog')

    const local = {
      request: {
        name: 'Heimnetz',
        type: 's3',
        config: {
          endpoint: rustfs.endpoint,
          region: rustfs.region,
          bucket: PROPOSED_BUCKET,
        },
      },
    }
    const asked = await probeRequest(
      page,
      probe,
      'extension_remote_storage_add_backend',
      local,
    )
    assert.equal(asked.error?.code, 1004)
    const host = new URL(rustfs.endpoint).host
    assert.deepEqual(asked.error?.details, {
      resourceType: 'remoteStorage',
      action: 'add',
      target: host,
    })
    await page.waitForDisplayed('extension-permission-request', 10_000)
    await page.click('extension-permission-allow')
    await begin(page, probe, 'extension_remote_storage_add_backend', local)
    await page.waitForDisplayed('storage-dialog-local', 10_000)
    await page.waitForDisplayed('storage-dialog-insecure')
    await page.click('storage-dialog-cancel')
    const cancelled = await answer(page, probe)
    assert.equal(cancelled.error?.code, 1002)
    ctx.step('a local endpoint after the add permission for its host, marked')
  },
)
