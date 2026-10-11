import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { fileFixture } from '../lib/file-fixtures.ts'
import { createAndUnlock, type FlowInstance } from '../lib/flows.ts'
import { contextMenu } from '../lib/passwords.ts'
import { solidPng } from '../lib/png.ts'
import {
  getObject,
  listKeys,
  listObjects,
  putObject,
  startRustfs,
} from '../lib/rustfs.ts'
import { scenario } from '../lib/scenario.ts'
import { connectStorage, filesGo } from '../lib/storage-setup.ts'
import { runAction } from '../lib/settings.ts'

// Spec 048, quickstart §2 and §3 (T031, T035): an encrypted folder in a storage of RustFS. It is
// created after the one-time notice, files and a folder go in from the device, the browser lists
// and opens them, and the bucket holds no name, path or content in the clear: only the head `h`,
// sidecars `m/…` and content objects `c/…` under a random prefix. Then one content object is
// changed and another cut short at the provider: both files are refused as damaged, the others
// still open. Needs Docker for RustFS; skipped without it.

const BUCKET = 'holzi-encrypted-e2e'
const NOTE = 'Hallo aus dem verschlüsselten Ordner'
const SECRET_NAMES = ['Unterlagen', 'notiz', 'brief', 'foto', 'Belege']
/** Length of a content object of a file of `size` bytes below 64 KiB: header, one tag, data. */
const contentLength = (size: number) => 38 + 16 + size

async function setName(instance: FlowInstance, name: string) {
  await instance.exec(
    `const input = document.querySelector('[data-testid="files-name-input"]')
     input.value = ''
     input.dispatchEvent(new Event('input', { bubbles: true }))
     return true`,
  )
  await instance.type('files-name-input', name)
  await instance.click('files-name-save')
}

const textOf = (instance: FlowInstance, hook: string) =>
  instance.exec<string>(
    `const el = document.querySelector('[data-testid="' + arguments[0] + '"]')
     return el ? el.textContent : ''`,
    [hook],
  )

scenario(
  'files-encrypted',
  { timeoutMs: 360_000, needs: { container: true } },
  async (ctx) => {
    const rustfs = await startRustfs([BUCKET])
    ctx.onTeardown(() => rustfs.stop())
    ctx.step('RustFS')

    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-encrypted' })
    const device = deviceFiles('holzi-encrypted-')
    ctx.onTeardown(() => device.remove())
    const png = solidPng(64, 48, [40, 120, 220])
    device.write('notiz.txt', NOTE)
    device.write('brief.pdf', fileFixture('brief.pdf'))
    device.write('foto.png', png)
    // Belege/d0/f0-0.txt (empty): a folder with a sub folder and a file.
    device.tree('Belege', 1, 1)
    const storageId = await connectStorage(instance, rustfs, BUCKET, 'Fotos')

    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/storage/${storageId}?p=${encodeURIComponent('/')}`,
    })
    await instance.waitForDisplayed('files-entries')

    // The first encrypted folder of this device: the notice, then the name.
    await contextMenu(instance, 'files-entries')
    await instance.click('files-menu-newEncryptedFolder')
    await instance.waitForDisplayed('files-encrypted-notice')
    await instance.click('files-encrypted-notice-confirm')
    await setName(instance, 'Unterlagen')
    await instance.waitForDisplayed('files-entry-Unterlagen')
    const created = await listKeys(rustfs, BUCKET)
    assert.equal(created.length, 1, created.join(', '))
    assert.match(created[0] ?? '', /^[a-z2-7]{26}\.hxef\/h$/)
    const prefix = (created[0] ?? '').slice(0, -1)
    ctx.step('created after the notice: one head under a random prefix')

    // Files and a folder go in from the device, one by one through holzi's clipboard.
    for (const name of ['notiz.txt', 'brief.pdf', 'foto.png', 'Belege']) {
      await filesGo(instance, `/device?p=${encodeURIComponent(device.folder)}`)
      await instance.waitForDisplayed(`files-entry-${name}`)
      await contextMenu(instance, `files-entry-${name}`)
      await instance.click('files-menu-copy')
      await instance.click('files-storage-Fotos')
      await instance.waitForDisplayed('files-entry-Unterlagen')
      await instance.click('files-entry-Unterlagen')
      await instance.click('files-action-paste')
      await instance.waitForDisplayed(`files-entry-${name}`)
    }
    await instance.click('files-entry-Belege')
    await instance.click('files-entry-d0')
    await instance.waitForDisplayed('files-entry-f0-0.txt')
    await instance.click('files-up')
    await instance.click('files-up')
    await instance.waitForDisplayed('files-entry-notiz.txt')
    ctx.step('files and a folder went in')

    // The bucket: only h, m/… and c/… under the prefix, no name and no content in the clear.
    const objects = await listObjects(rustfs, BUCKET)
    for (const { key } of objects) {
      assert.ok(key.startsWith(prefix), key)
      assert.match(key.slice(prefix.length), /^(h|[mc]\/[a-z2-7]{26})$/, key)
      for (const secret of SECRET_NAMES)
        assert.ok(!key.toLowerCase().includes(secret.toLowerCase()), key)
    }
    for (const { key } of objects) {
      const body = Buffer.from(await getObject(rustfs, BUCKET, key))
      for (const secret of [...SECRET_NAMES, NOTE])
        assert.ok(!body.includes(secret), `${secret} in ${key}`)
    }
    // Four files and two folders: six sidecars, four content objects.
    const under = (kind: string) =>
      objects.filter(({ key }) => key.startsWith(`${prefix}${kind}/`))
    assert.equal(
      under('m').length,
      6,
      'sidecars of notiz, brief, foto, Belege, d0, f0-0',
    )
    assert.equal(under('c').length, 4, 'content of notiz, brief, foto, f0-0')
    ctx.step('the bucket holds nothing in the clear')

    // The browser opens text, PDF and image, and shows a thumbnail in the grid.
    await instance.click('files-entry-notiz.txt')
    await instance.waitForDisplayed('files-viewer-text')
    assert.match(await textOf(instance, 'files-viewer-text'), new RegExp(NOTE))
    await instance.click('files-viewer-close')
    await instance.click('files-entry-brief.pdf')
    await instance.waitForDisplayed('files-viewer-pdf')
    await instance.click('files-viewer-close')
    await instance.click('files-entry-foto.png')
    await instance.waitForDisplayed('files-viewer-image')
    await instance.click('files-viewer-close')
    await instance.click('files-view-toggle')
    await ctx.waitFor('the thumbnail of foto.png', () =>
      instance.exec<boolean>(
        `const img = document.querySelector('[data-testid="files-entry-foto.png"] img')
         return Boolean(img && img.complete && img.naturalWidth > 0)`,
      ),
    )
    await instance.click('files-view-toggle')
    ctx.step('text, PDF and image open from the encrypted folder')

    // At the provider: one byte of notiz.txt changed, foto.png cut short.
    const note = under('c').find(
      ({ size }) => size === contentLength(Buffer.byteLength(NOTE)),
    )
    const photo = under('c').find(
      ({ size }) => size === contentLength(png.length),
    )
    assert.ok(note && photo, 'content objects of notiz.txt and foto.png')
    const changed = await getObject(rustfs, BUCKET, note.key)
    changed[changed.length - 20] ^= 0x01
    await putObject(rustfs, BUCKET, note.key, changed)
    const cut = await getObject(rustfs, BUCKET, photo.key)
    await putObject(rustfs, BUCKET, photo.key, cut.slice(0, cut.length - 10))
    await instance.click('files-reload')
    await instance.waitForDisplayed('files-entry-notiz.txt')

    await instance.click('files-entry-notiz.txt')
    await instance.waitForDisplayed('files-viewer-failed')
    assert.doesNotMatch(
      await textOf(instance, 'files-viewer'),
      new RegExp(NOTE),
    )
    await instance.click('files-viewer-close')
    await instance.click('files-entry-foto.png')
    await instance.waitForDisplayed('files-viewer-failed')
    await instance.click('files-viewer-close')
    await instance.click('files-entry-brief.pdf')
    await instance.waitForDisplayed('files-viewer-pdf')
    await instance.click('files-viewer-close')
    ctx.step('changed and cut content is refused, the rest still opens')
  },
)
