import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { fileFixture } from '../lib/file-fixtures.ts'
import { createAndUnlock, type FlowInstance } from '../lib/flows.ts'
import { contextMenu, doubleClick } from '../lib/passwords.ts'
import { listKeys, putObject, startRustfs } from '../lib/rustfs.ts'
import { scenario } from '../lib/scenario.ts'
import { connectStorage, filesGo } from '../lib/storage-setup.ts'
import { runAction } from '../lib/settings.ts'

// Spec 044, US5, quickstart §5 (T068): a storage of RustFS in the file browser. Its prefixes are
// folders, a new folder is a marker object, renaming a folder moves every object under it, a text
// shows and a film plays and seeks from the storage through the media server, a search with a typo
// finds a file in it, a file goes up from the device (holzi's clipboard across tabs), and deleting
// asks first (no trash) and leaves nothing of the folder. Downloading is `files-storage-down`.
// The bucket is checked directly each time.
// Needs Docker for RustFS; skipped without it.

const BUCKET = 'holzi-files-e2e'

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
  'files-storage',
  { timeoutMs: 300_000, needs: { container: true } },
  async (ctx) => {
    const rustfs = await startRustfs([BUCKET])
    ctx.onTeardown(() => rustfs.stop())
    await putObject(rustfs, BUCKET, 'notiz.txt', 'Hallo aus dem Speicher')
    await putObject(rustfs, BUCKET, 'film.mp4', fileFixture('film.mp4'))
    await putObject(rustfs, BUCKET, 'Alt/a.txt', 'a')
    await putObject(rustfs, BUCKET, 'Alt/tief/b.txt', 'b')
    ctx.step('RustFS with objects')

    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-storage' })
    // After the start: on Android it clears the app's data, where the folder lives.
    const device = deviceFiles('holzi-storage-')
    ctx.onTeardown(() => device.remove())
    device.write('hoch.txt', 'vom Gerät')
    const storageId = await connectStorage(instance, rustfs, BUCKET, 'Fotos')

    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/storage/${storageId}?p=${encodeURIComponent('/')}`,
    })
    await instance.waitForDisplayed('files-entry-Alt')
    await instance.waitForDisplayed('files-entry-notiz.txt')
    await instance.waitForDisplayed('files-storage-Fotos')
    ctx.step('the storage lists its prefixes as folders')

    // A new folder is a marker object.
    await contextMenu(instance, 'files-entries')
    await instance.click('files-menu-newFolder')
    await setName(instance, 'Neu')
    await instance.waitForDisplayed('files-entry-Neu')
    assert.ok((await listKeys(rustfs, BUCKET)).includes('Neu/'))

    // Renaming a folder moves every object under it.
    await contextMenu(instance, 'files-entry-Alt')
    await instance.click('files-menu-rename')
    await setName(instance, 'Archiv')
    await instance.waitForDisplayed('files-entry-Archiv')
    const renamed = await listKeys(rustfs, BUCKET)
    assert.ok(
      renamed.includes('Archiv/a.txt') && renamed.includes('Archiv/tief/b.txt'),
    )
    assert.ok(
      !renamed.some((key) => key.startsWith('Alt/')),
      renamed.join(', '),
    )
    ctx.step('a new folder and a renamed one')

    // A text and a film from the storage.
    await doubleClick(instance, 'files-entry-notiz.txt')
    await instance.waitForDisplayed('files-viewer-text')
    assert.match(
      await textOf(instance, 'files-viewer-text'),
      /Hallo aus dem Speicher/,
    )
    await instance.click('files-viewer-close')
    await doubleClick(instance, 'files-entry-film.mp4')
    await instance.waitForDisplayed('files-viewer-video')
    await ctx.waitFor('the film to play and seek', () =>
      instance.exec<boolean>(
        `const el = document.querySelector('[data-testid="files-viewer-video"]')
         el.muted = true
         if (el.paused) void el.play().catch(() => {})
         if (el.currentTime > 0.3 && !el.seeking && el.currentTime < 5) el.currentTime = 6
         return !el.seeking && el.currentTime >= 6 && el.readyState >= 2`,
      ),
    )
    await instance.click('files-viewer-close')
    ctx.step('a text shows and a film plays and seeks from the storage')

    // A search with a typo finds a file of the storage.
    await instance.type('files-search', 'notz')
    await instance.waitForDisplayed('files-hit-notiz.txt')
    await instance.click('files-search-clear')
    await instance.waitForDisplayed('files-entry-notiz.txt')
    ctx.step('a search in the storage')

    // Up: a file of the device into the storage, through holzi's clipboard.
    await filesGo(instance, `/device?p=${encodeURIComponent(device.folder)}`)
    await instance.waitForDisplayed('files-entry-hoch.txt')
    await contextMenu(instance, 'files-entry-hoch.txt')
    await instance.click('files-menu-copy')
    await instance.click('files-storage-Fotos')
    await instance.waitForDisplayed('files-entry-notiz.txt')
    await instance.click('files-action-paste')
    await instance.waitForDisplayed('files-entry-hoch.txt')
    assert.ok((await listKeys(rustfs, BUCKET)).includes('hoch.txt'))
    ctx.step('a file went up')

    // Deleting asks first: a storage has no trash.
    await contextMenu(instance, 'files-entry-Archiv')
    await instance.click('files-menu-delete')
    await instance.waitForDisplayed('files-delete-confirm')
    await instance.click('files-delete-confirm')
    await ctx.waitFor('Archiv to go', async () =>
      instance.exec<boolean>(
        `return !document.querySelector('[data-testid="files-entry-Archiv"]')`,
      ),
    )
    const left = await listKeys(rustfs, BUCKET)
    assert.ok(!left.some((key) => key.startsWith('Archiv/')), left.join(', '))
    ctx.step('deleted after the question')
  },
)
