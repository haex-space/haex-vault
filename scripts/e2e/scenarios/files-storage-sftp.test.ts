import assert from 'node:assert/strict'

import { textOf } from '../lib/fields.ts'
import { fileFixture } from '../lib/file-fixtures.ts'
import { createAndUnlock, type FlowInstance } from '../lib/flows.ts'
import { contextMenu, doubleClick } from '../lib/passwords.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'
import { prepareSftp } from '../lib/sftp.ts'
import { connectSftp } from '../lib/storage-setup.ts'

// Spec 050, US5 (T085): an SFTP storage in the file browser, like the WebDAV storage of
// `files-storage-webdav`. A new folder is made on the server and a name that differs only in case
// is taken; a folder is renamed and deleted with one call each; a film plays and seeks through
// range reads; a file is copied inside the storage, streaming through holzi because SFTP has no
// copy on the server; no temporary file stays. Cancelling an upload is checked against a server
// in the test process (`files/transfer/remote_sftp_tests.rs`). Needs Docker; skipped without it.

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

scenario(
  'files-storage-sftp',
  { timeoutMs: 300_000, needs: { container: true } },
  async (ctx) => {
    const sftp = await prepareSftp({ password: 'sftp-e2e-password' })
    ctx.onTeardown(() => sftp.stop())
    sftp.writeFile(`${FOLDER}/notiz.txt`, 'Hallo vom SFTP-Server')
    sftp.writeFile(`${FOLDER}/film.mp4`, fileFixture('film.mp4'))
    sftp.writeFile(`${FOLDER}/Alt/a.txt`, 'a')
    sftp.writeFile(`${FOLDER}/Alt/tief/b.txt`, 'b')
    await sftp.startAsync()
    const tree = () => sftp.listTree(FOLDER)
    ctx.step('SFTP server with files')

    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-sftp' })
    const storageId = await connectSftp(instance, sftp, FOLDER, 'Server')
    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/storage/${storageId}?p=${encodeURIComponent('/')}`,
    })
    await instance.waitForDisplayed('files-entry-Alt')
    await instance.waitForDisplayed('files-entry-notiz.txt')
    ctx.step('the storage lists the folders of the server')

    await contextMenu(instance, 'files-entries')
    await instance.click('files-menu-newFolder')
    await setName(instance, 'Neu')
    await instance.waitForDisplayed('files-entry-Neu')
    assert.ok(tree().includes('Neu/'))
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
    assert.ok(!tree().includes('neu/'))
    ctx.step('a new folder, and a name differing only in case is taken')

    await contextMenu(instance, 'files-entry-Alt')
    await instance.click('files-menu-rename')
    await setName(instance, 'Archiv')
    await instance.waitForDisplayed('files-entry-Archiv')
    const renamed = tree()
    assert.ok(
      renamed.includes('Archiv/a.txt') && renamed.includes('Archiv/tief/b.txt'),
      renamed.join(', '),
    )
    ctx.step('a renamed folder')

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
    ctx.step('a film plays and seeks from the server')

    // A copy inside the storage streams through holzi (research R8).
    await contextMenu(instance, 'files-entry-notiz.txt')
    await instance.click('files-menu-copy')
    await doubleClick(instance, 'files-entry-Neu')
    await instance.waitForDisplayed('files-empty')
    await instance.click('files-action-paste')
    await instance.waitForDisplayed('files-entry-notiz.txt')
    await ctx.waitFor('the copy on the server', async () =>
      tree().includes('Neu/notiz.txt'),
    )
    assert.ok(tree().includes('notiz.txt'), 'the source stays')
    ctx.step('a file copied inside the storage')

    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/storage/${storageId}?p=${encodeURIComponent('/')}`,
    })
    await instance.waitForDisplayed('files-entry-Archiv')
    await contextMenu(instance, 'files-entry-Archiv')
    await instance.click('files-menu-delete')
    await instance.waitForDisplayed('files-delete-confirm')
    await instance.click('files-delete-confirm')
    await ctx.waitFor('Archiv to go', async () =>
      instance.exec<boolean>(
        `return !document.querySelector('[data-testid="files-entry-Archiv"]')`,
      ),
    )
    const left = tree()
    assert.ok(!left.some((path) => path.startsWith('Archiv')), left.join(', '))
    assert.ok(
      !left.some((path) => path.includes('.holzi-')),
      `no temporary file stays: ${left.join(', ')}`,
    )
    ctx.step('deleted after the question, nothing of holzi left')
  },
)
