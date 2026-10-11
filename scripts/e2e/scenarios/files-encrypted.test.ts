import assert from 'node:assert/strict'
import { deviceFiles } from '../lib/extension-files.ts'
import { fileFixture } from '../lib/file-fixtures.ts'
import {
  createAndUnlock,
  openLauncher,
  type FlowInstance,
} from '../lib/flows.ts'
import { PROCESS_END_LIMIT_MS } from '../lib/close-promises.ts'
import { contextMenu } from '../lib/passwords.ts'
import { solidPng } from '../lib/png.ts'
import {
  getObject,
  listKeys,
  listObjects,
  putObject,
  type Rustfs,
  startRustfs,
} from '../lib/rustfs.ts'
import { scenario, type ScenarioContext } from '../lib/scenario.ts'
import { connectStorage, filesGo } from '../lib/storage-setup.ts'
import { encryptedSystemAppFiles } from '../lib/system-app-files.ts'
import { runAction } from '../lib/settings.ts'

// Spec 048, quickstart §2 and §3 (T031, T035): an encrypted folder in a storage of RustFS. It is
// created after the one-time notice, files and a folder go in from the device, the browser lists
// and opens them, and the bucket holds no name, path or content in the clear: only the head `h`,
// sidecars `m/…` and content objects `c/…` under a random prefix. Then one content object is
// changed and another cut short at the provider: both files are refused as damaged, the others
// still open. Part 4 (quickstart §6, §9, T049) tidies up inside: a renamed folder of 200 files
// writes no content object, a move and a copy, a delete, a search by a decrypted name, a copy into
// an ordinary folder that asks first, "Mit System-App öffnen" behind its notice, and nothing of
// the copies for system apps left after the vault closed. Needs Docker for RustFS; skipped
// without it.

const BUCKET = 'holzi-encrypted-e2e'
const NOTE = 'Hallo aus dem verschlüsselten Ordner'
const SECRET_NAMES = ['Unterlagen', 'notiz', 'brief', 'foto', 'Belege']
// Android's emulator needs several minutes for the deliberately large folder copy below.
const TRANSFER_TIMEOUT_MS = 300_000
/** Length of a content object of a file of `size` bytes below 64 KiB: header, one tag, data. */
const contentLength = (size: number) => 38 + 16 + size

async function setName(instance: FlowInstance, name: string) {
  await instance.waitForDisplayed('files-name-input')
  await instance.exec(
    `const input = document.querySelector('[data-testid="files-name-input"]')
     input.value = ''
     input.dispatchEvent(new Event('input', { bubbles: true }))
     return true`,
  )
  await instance.type('files-name-input', name)
  await instance.click('files-name-save')
}

const gone = (instance: FlowInstance, hook: string) =>
  instance.exec<boolean>(
    `return !document.querySelector('[data-testid="' + arguments[0] + '"]')`,
    [hook],
  )

/** The keys under the encrypted folder's prefix of one kind (`m` or `c`). */
async function keysUnder(rustfs: Rustfs, prefix: string, kind: 'm' | 'c') {
  return (await listKeys(rustfs, BUCKET))
    .filter((key) => key.startsWith(`${prefix}${kind}/`))
    .sort()
}

/** Pastes holzi's clipboard into the open folder and waits for the transfer to end. */
async function pasteAndWait(ctx: ScenarioContext, instance: FlowInstance) {
  await instance.click('files-action-paste')
  await ctx.waitFor(
    'the transfer to end',
    () => gone(instance, 'files-transfers'),
    { timeoutMs: TRANSFER_TIMEOUT_MS },
  )
}

/** Answers the question before deleting for good in a storage. */
async function deleteEntry(instance: FlowInstance, name: string) {
  await contextMenu(instance, `files-entry-${name}`)
  await instance.click('files-menu-delete')
  await instance.click('files-delete-confirm')
}

const textOf = (instance: FlowInstance, hook: string) =>
  instance.exec<string>(
    `const el = document.querySelector('[data-testid="' + arguments[0] + '"]')
     return el ? el.textContent : ''`,
    [hook],
  )

scenario(
  'files-encrypted',
  { timeoutMs: 600_000, needs: { container: true } },
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
    await instance.waitForDisplayed('files-empty')

    // The first encrypted folder of this device: the notice, then the name.
    await contextMenu(instance, 'files-empty')
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

    // Part 4. A folder of 200 files and strom.pdf goes in (SC-006 asks for 1 000; 200 keep the
    // CI's time, and a rename writes one sidecar however many files the folder holds).
    device.tree('Rechnungen', 1, 200)
    device.write('Rechnungen/d0/strom.pdf', fileFixture('brief.pdf'))
    await filesGo(instance, `/device?p=${encodeURIComponent(device.folder)}`)
    await instance.waitForDisplayed('files-entry-Rechnungen')
    await contextMenu(instance, 'files-entry-Rechnungen')
    await instance.click('files-menu-copy')
    await instance.click('files-storage-Fotos')
    await instance.waitForDisplayed('files-entry-Unterlagen')
    await instance.click('files-entry-Unterlagen')
    await instance.waitForDisplayed('files-entry-brief.pdf')
    await pasteAndWait(ctx, instance)
    await instance.waitForDisplayed('files-entry-Rechnungen')
    ctx.step('a folder of 200 files went in')

    // Renaming it writes one sidecar and deletes the old one; no content object changes.
    const contentBefore = await keysUnder(rustfs, prefix, 'c')
    const sidecarsBefore = (await keysUnder(rustfs, prefix, 'm')).length
    const renameStart = performance.now()
    await contextMenu(instance, 'files-entry-Rechnungen')
    await instance.click('files-menu-rename')
    await setName(instance, 'Archiv')
    await instance.waitForDisplayed('files-entry-Archiv', 30_000)
    const renameMs = Math.round(performance.now() - renameStart)
    assert.ok(renameMs < 30_000, `rename took ${renameMs} ms`)
    assert.deepEqual(await keysUnder(rustfs, prefix, 'c'), contentBefore)
    assert.equal((await keysUnder(rustfs, prefix, 'm')).length, sidecarsBefore)
    ctx.step('renamed: no content object written', `${renameMs} ms`)

    // A move inside the folder: no content object either.
    await contextMenu(instance, 'files-entry-brief.pdf')
    await instance.click('files-menu-cut')
    await instance.click('files-entry-Belege')
    await instance.waitForDisplayed('files-entry-d0')
    await pasteAndWait(ctx, instance)
    await instance.waitForDisplayed('files-entry-brief.pdf')
    assert.deepEqual(await keysUnder(rustfs, prefix, 'c'), contentBefore)
    ctx.step('moved into Belege')

    // A copy back up: the provider copies the content object under a new name.
    await contextMenu(instance, 'files-entry-brief.pdf')
    await instance.click('files-menu-copy')
    await instance.click('files-up')
    await instance.waitForDisplayed('files-entry-Archiv')
    await pasteAndWait(ctx, instance)
    await instance.waitForDisplayed('files-entry-brief.pdf')
    assert.equal(
      (await keysUnder(rustfs, prefix, 'c')).length,
      contentBefore.length + 1,
    )
    await instance.click('files-entry-brief.pdf')
    await instance.waitForDisplayed('files-viewer-pdf')
    await instance.click('files-viewer-close')
    ctx.step('copied: the copy opens')

    // The damaged notiz.txt can be deleted; its content object goes with it.
    await deleteEntry(instance, 'notiz.txt')
    await ctx.waitFor('notiz.txt to go', () =>
      gone(instance, 'files-entry-notiz.txt'),
    )
    assert.ok(!(await keysUnder(rustfs, prefix, 'c')).includes(note.key))
    ctx.step('deleted')

    // The search finds strom.pdf by its decrypted name.
    await instance.exec(
      `const input = document.querySelector('[data-testid="files-search"]')
       input.value = 'strom'
       input.dispatchEvent(new Event('input', { bubbles: true }))
       return true`,
    )
    await instance.waitForDisplayed('files-hit-strom.pdf', 20_000)
    await instance.click('files-search-clear')
    await instance.waitForDisplayed('files-entry-Archiv')
    ctx.step('the search finds strom.pdf')

    // Into an ordinary folder of the storage it asks first; there the file lies in the clear.
    await contextMenu(instance, 'files-entry-brief.pdf')
    await instance.click('files-menu-copy')
    await instance.click('files-up')
    await instance.waitForDisplayed('files-entry-Unterlagen')
    await instance.click('files-action-paste')
    await instance.waitForDisplayed('files-leave-encryption')
    await instance.click('files-leave-encryption-proceed')
    await ctx.waitFor('the transfer to end', () =>
      gone(instance, 'files-transfers'),
    )
    await instance.waitForDisplayed('files-entry-brief.pdf')
    assert.ok((await listKeys(rustfs, BUCKET)).includes('brief.pdf'))
    ctx.step('a copy into an ordinary folder asked first')

    // "Mit System-App öffnen" shows its notice; confirmed, the copy lies in its own place.
    const appFiles = encryptedSystemAppFiles(instance.root)
    await instance.click('files-entry-Unterlagen')
    await instance.waitForDisplayed('files-entry-Archiv')
    await contextMenu(instance, 'files-entry-brief.pdf')
    await instance.click('files-menu-openSystem')
    await instance.waitForDisplayed('files-open-system-notice')
    await instance.click('files-open-system-confirm')
    // Whether a system app is there to open it does not matter here: the copy shows that the
    // command ran with the confirmation.
    await ctx.waitFor('the decrypted copy for the system app', () =>
      appFiles.hasOpenedCopy(),
    )
    ctx.step('open with the system app after the notice')

    // Closing the vault removes the copies; the app's log names nothing of the folder (SC-009).
    await openLauncher(instance)
    await instance.press('lock-instance')
    await instance.waitForEnd(PROCESS_END_LIMIT_MS)
    assert.ok(appFiles.hasNoOpenedCopy(), 'no copy left after the close')
    const logTexts = appFiles.logTexts()
    for (const text of logTexts) {
      for (const secret of [...SECRET_NAMES, 'Rechnungen', 'Archiv', 'strom'])
        assert.ok(!text.includes(secret), `${secret} in a log`)
    }
    ctx.step(
      'closed: no copy left, nothing named in the log',
      `${logTexts.length} log files`,
    )
  },
)
