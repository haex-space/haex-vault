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
import { connectStorage } from '../lib/storage-setup.ts'
import type { Page } from '../lib/page.ts'

const BUCKET = 'holzi-ext-encrypted'

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

function shown(page: Page, testid: string): Promise<boolean> {
  return page.exec<boolean>(
    `return !!document.querySelector('[data-testid="' + arguments[0] + '"]')`,
    [testid],
  )
}

function text(page: Page, testid: string): Promise<string> {
  return page.exec<string>(
    `return document.querySelector('[data-testid="' + arguments[0] + '"]')?.textContent ?? ''`,
    [testid],
  )
}

// Spec 048, US6 extension part (quickstart §8, T065): an extension without `remoteStorage` is
// refused without a question; with it, holzi's chooser names the folders and asks in the same
// view; an answer without "Erlaubnis merken" holds until the extension reloads, one with it is not
// asked again; a revoke holds at once. The forbidden encrypted-folder manifest case is covered by
// the Rust registry/developer-mode tests because developer project folders are not available on
// Android. The probe fixture is not rebuilt: it calls the bridge by name like the storage scene,
// and permissions are set from the scene. Needs Docker for RustFS; skipped without it.
scenario(
  'extension-encrypted-folder',
  { timeoutMs: 300_000, needs: { container: true } },
  async (ctx) => {
    const rustfs = await startRustfs([BUCKET])
    ctx.onTeardown(() => rustfs.stop())
    ctx.step('RustFS started')
    const group = await ctx.group({ users: { anna: ['laptop'] } })
    const page = group.device('anna/laptop').page

    const storageId = await connectStorage(page, rustfs, BUCKET, 'Fotos')
    unwrap(
      'files_create_encrypted_folder',
      await page.invoke('files_create_encrypted_folder', {
        source: { kind: 'storage', storageId },
        path: '/',
        name: 'Unterlagen',
      }),
    )
    const probe = await install(page, fixture('e2e', 'probe'))
    await openFromLauncher(page, probe)
    const ready = () =>
      inFrame<boolean>(
        page,
        probe,
        `return document.documentElement.dataset.probeReady === '1'`,
      ).catch(() => false)
    await ctx.waitFor('the SDK channel of the probe', ready, {
      timeoutMs: 20_000,
    })
    ctx.step('a storage with the encrypted folder "Unterlagen"')

    const choose = {
      request: { storageId, access: 'readWrite' },
    }
    const without = await probeRequest(
      page,
      probe,
      'extension_encrypted_folder_choose',
      choose,
    )
    assert.equal(without.error?.code, 1002, JSON.stringify(without))
    assert.ok(!(await shown(page, 'extension-encrypted-chooser')))
    assert.ok(!(await shown(page, 'extension-permission-request')))
    ctx.step('without remoteStorage: refused, nobody asked')

    unwrap(
      'extension_permission_set',
      await page.invoke('extension_permission_set', {
        args: {
          extensionId: probe.id,
          kind: 'remoteStorage',
          action: 'readWrite',
          target: storageId,
          status: 'granted',
        },
      }),
    )
    await begin(page, probe, 'extension_encrypted_folder_choose', choose)
    await page.waitForDisplayed(
      'extension-encrypted-chooser-folder-Unterlagen',
      10_000,
    )
    assert.match(
      await text(page, 'extension-encrypted-chooser'),
      /Fotos/,
      'the chooser names the storage',
    )
    await page.click('extension-encrypted-chooser-read-write')
    const chosen = await answer(page, probe)
    assert.equal(chosen.error, undefined, JSON.stringify(chosen))
    const { folderId, name } = chosen.result as {
      folderId: string
      name: string
    }
    assert.equal(name, 'Unterlagen')
    ctx.step('the chooser asked; read and write allowed without remembering')

    const at = (path: string) => ({ request: { storageId, folderId, path } })
    const put = await probeRequest(
      page,
      probe,
      'extension_encrypted_folder_upload',
      {
        request: {
          ...at('notiz.txt').request,
          data: Buffer.from('hallo').toString('base64'),
        },
      },
    )
    assert.equal(put.error, undefined, JSON.stringify(put))
    const got = await probeRequest(
      page,
      probe,
      'extension_encrypted_folder_download',
      at('notiz.txt'),
    )
    assert.equal(Buffer.from(String(got.result), 'base64').toString(), 'hallo')
    const listed = await probeRequest(
      page,
      probe,
      'extension_encrypted_folder_list',
      at(''),
    )
    assert.deepEqual(
      (listed.result as { entries: { name: string }[] }).entries.map(
        (e) => e.name,
      ),
      ['notiz.txt'],
    )
    const keys = await listKeys(rustfs, BUCKET)
    assert.ok(
      keys.every((key) =>
        /^[a-z2-7]{26}\.hxef\/(h|[mc]\/[a-z2-7]{26})$/.test(key),
      ),
      keys.join(', '),
    )
    assert.ok(!JSON.stringify(listed).includes('.hxef'))
    ctx.step('decrypted round trip; the bucket holds only encrypted objects')

    // A reload of the extension ends the answer that was not remembered.
    await inFrame(page, probe, `location.reload(); return true`).catch(() => {})
    let asked: BridgeAnswer | undefined
    await ctx.waitFor(
      'the question after the reload',
      async () => {
        if (!(await ready())) return false
        asked = await probeRequest(
          page,
          probe,
          'extension_encrypted_folder_list',
          at(''),
        )
        return asked.error?.code === 1004
      },
      { timeoutMs: 20_000 },
    )
    assert.ok(!JSON.stringify(asked).includes('Unterlagen'))
    await page.waitForDisplayed('extension-permission-request', 10_000)
    assert.match(
      await text(page, 'extension-permission-request'),
      /Unterlagen.*Fotos/s,
    )
    assert.equal(
      await page.exec<string>(
        `return document.querySelector('[data-testid="extension-permission-remember"]')?.getAttribute('data-state') ?? ''`,
      ),
      'unchecked',
      '"Erlaubnis merken" starts off',
    )
    await page.click('extension-permission-remember')
    await page.click('extension-permission-allow')
    ctx.step('asked again after the reload; remembered this time')

    await inFrame(page, probe, `location.reload(); return true`).catch(() => {})
    await ctx.waitFor(
      'the remembered answer after another reload',
      async () =>
        (await ready()) &&
        (
          await probeRequest(
            page,
            probe,
            'extension_encrypted_folder_list',
            at(''),
          )
        ).error === undefined,
      { timeoutMs: 20_000 },
    )
    assert.ok(!(await shown(page, 'extension-permission-request')))
    ctx.step('remembered: no question after a reload')

    const views = unwrap<
      { id?: string; kind: string; target: string; folder?: unknown }[]
    >(
      'extension_permissions_list',
      await page.invoke('extension_permissions_list', {
        extensionId: probe.id,
      }),
    )
    const grant = views.find((v) => v.kind === 'encryptedFolder' && v.id)
    assert.ok(grant, JSON.stringify(views))
    assert.deepEqual(grant.folder, {
      storageName: 'Fotos',
      folderName: 'Unterlagen',
      onDevice: false,
    })
    unwrap(
      'extension_permission_remove',
      await page.invoke('extension_permission_remove', {
        args: { extensionId: probe.id, permissionId: grant.id },
      }),
    )
    const revoked = await probeRequest(
      page,
      probe,
      'extension_encrypted_folder_list',
      at(''),
    )
    assert.equal(revoked.error?.code, 1004, JSON.stringify(revoked))
    ctx.step('a revoke holds at once')
  },
)
