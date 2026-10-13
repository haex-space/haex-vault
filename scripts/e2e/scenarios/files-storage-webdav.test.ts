import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { fileFixture } from '../lib/file-fixtures.ts'
import { createAndUnlock, type FlowInstance } from '../lib/flows.ts'
import { contextMenu, doubleClick } from '../lib/passwords.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'
import { connectWebdav, filesGo } from '../lib/storage-setup.ts'
import { listTree, putFile, startWebdav } from '../lib/webdav.ts'

// Spec 050, US2, quickstart §4 (T047): a WebDAV storage in the file browser, like the S3 storage of
// `files-storage`. Its folders are real folders: a new one is made on the server, a name that
// differs only in case is taken, renaming and deleting a folder take one call each. A text shows
// and a film plays and seeks, a file goes up from the device and a folder comes down, and no
// temporary file stays on the server. Cancelling an upload is checked against a server in the test
// process (`webdav/server_tests.rs`), where its timing is sure. Needs Docker; skipped without it.

const FOLDER = 'Ablage'

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
  'files-storage-webdav',
  { timeoutMs: 300_000, needs: { container: true } },
  async (ctx) => {
    const webdav = await startWebdav()
    ctx.onTeardown(() => webdav.stop())
    await putFile(webdav, `${FOLDER}/notiz.txt`, 'Hallo vom Server')
    await putFile(webdav, `${FOLDER}/film.mp4`, fileFixture('film.mp4'))
    await putFile(webdav, `${FOLDER}/Alt/a.txt`, 'a')
    await putFile(webdav, `${FOLDER}/Alt/tief/b.txt`, 'b')
    const tree = () => listTree(webdav, `/${FOLDER}/`)
    ctx.step('WebDAV server with files')

    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-webdav' })
    const device = deviceFiles('holzi-webdav-')
    ctx.onTeardown(() => device.remove())
    device.write('hoch.txt', 'vom Gerät')
    const storageId = await connectWebdav(instance, webdav, FOLDER, 'Server')

    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/storage/${storageId}?p=${encodeURIComponent('/')}`,
    })
    await instance.waitForDisplayed('files-entry-Alt')
    await instance.waitForDisplayed('files-entry-notiz.txt')
    await instance.waitForDisplayed('files-storage-Server')
    ctx.step('the storage lists the folders of the server')

    // A new folder is a folder on the server; one that differs only in case is taken.
    await contextMenu(instance, 'files-entries')
    await instance.click('files-menu-newFolder')
    await setName(instance, 'Neu')
    await instance.waitForDisplayed('files-entry-Neu')
    assert.ok((await tree()).includes('Neu/'))
    await contextMenu(instance, 'files-entries')
    await instance.click('files-menu-newFolder')
    await setName(instance, 'neu')
    await instance.waitForDisplayed('files-name-error')
    assert.match(
      await textOf(instance, 'files-name-error'),
      /gibt es schon|already/,
    )
    await setName(instance, 'Leer')
    await instance.waitForDisplayed('files-entry-Leer')
    assert.ok(!(await tree()).includes('neu/'))
    ctx.step('new folders, and a name differing only in case is taken')

    // Renaming a folder moves it with everything in it.
    await contextMenu(instance, 'files-entry-Alt')
    await instance.click('files-menu-rename')
    await setName(instance, 'Archiv')
    await instance.waitForDisplayed('files-entry-Archiv')
    const renamed = await tree()
    assert.ok(
      renamed.includes('Archiv/a.txt') && renamed.includes('Archiv/tief/b.txt'),
      renamed.join(', '),
    )
    assert.ok(!renamed.some((path) => path.startsWith('Alt')))
    ctx.step('a renamed folder')

    // A text and a film from the server.
    await doubleClick(instance, 'files-entry-notiz.txt')
    await instance.waitForDisplayed('files-viewer-text')
    assert.match(
      await textOf(instance, 'files-viewer-text'),
      /Hallo vom Server/,
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
    ctx.step('a text shows and a film plays and seeks from the server')

    // Up: a file of the device onto the server, through holzi's clipboard.
    await filesGo(instance, `/device?p=${encodeURIComponent(device.folder)}`)
    await instance.waitForDisplayed('files-entry-hoch.txt')
    await contextMenu(instance, 'files-entry-hoch.txt')
    await instance.click('files-menu-copy')
    await instance.click('files-storage-Server')
    await instance.waitForDisplayed('files-entry-notiz.txt')
    await instance.click('files-action-paste')
    await instance.waitForDisplayed('files-entry-hoch.txt')
    assert.ok((await tree()).includes('hoch.txt'))
    ctx.step('a file went up')

    // Down: a folder of the server onto the device, its tree kept.
    await contextMenu(instance, 'files-entry-Archiv')
    await instance.click('files-menu-copy')
    await filesGo(instance, `/device?p=${encodeURIComponent(device.folder)}`)
    await instance.waitForDisplayed('files-entry-hoch.txt')
    await instance.click('files-action-paste')
    await ctx.waitFor('the tree to arrive', async () => {
      try {
        return device.read('Archiv/tief/b.txt') === 'b'
      } catch {
        return false
      }
    })
    assert.equal(device.read('Archiv/a.txt'), 'a')
    ctx.step('a folder came down')

    // Deleting asks first and takes the folder with everything in it.
    // The device folder has an `Archiv` now too: wait for an entry only the server has.
    await instance.click('files-storage-Server')
    await instance.waitForDisplayed('files-entry-notiz.txt')
    await contextMenu(instance, 'files-entry-Archiv')
    await instance.click('files-menu-delete')
    await instance.waitForDisplayed('files-delete-confirm')
    await instance.click('files-delete-confirm')
    await ctx.waitFor('Archiv to go', async () =>
      instance.exec<boolean>(
        `return !document.querySelector('[data-testid="files-entry-Archiv"]')`,
      ),
    )
    assert.equal(
      device.read('Archiv/a.txt'),
      'a',
      'the copy on the device stays',
    )
    const left = await tree()
    assert.ok(!left.some((path) => path.startsWith('Archiv')), left.join(', '))
    assert.ok(
      !left.some((path) => path.includes('.holzi-')),
      `no temporary file stays: ${left.join(', ')}`,
    )
    ctx.step('deleted after the question, nothing of holzi left')
  },
)
