import assert from 'node:assert/strict'
import { deviceFiles } from '../lib/extension-files.ts'
import { fileFixture } from '../lib/file-fixtures.ts'
import { createAndUnlock, type FlowInstance } from '../lib/flows.ts'
import { contextMenu, doubleClick } from '../lib/passwords.ts'
import { solidPng } from '../lib/png.ts'
import { scenario } from '../lib/scenario.ts'
import { filesGo } from '../lib/storage-setup.ts'
import { runAction } from '../lib/settings.ts'

// Spec 048 user story 7, quickstart §10 (T078): an encrypted folder in an ordinary folder of the
// device. It is created after the one-time notice; a text, an image and a film are copied in through
// holzi's clipboard and open from it; a rename and a search work by the decrypted names. On the disk
// the folder holds only `h`, `m/…` and `c/…` under a random name, nothing in the clear. A file
// another program puts there (a sync program's conflict copy) is no entry, the folder hints at it,
// and deleting the folder asks first, names it and takes it along: nothing goes to the trash.

const NOTE = 'Hallo aus dem verschlüsselten Ordner auf dem Gerät'
const SECRET_NAMES = ['Tresor', 'notiz', 'foto', 'film', 'Belege']
const TRANSFER_TIMEOUT_MS = 120_000

async function setName(instance: FlowInstance, name: string) {
  await instance.waitForDisplayed('files-name-input')
  await instance.exec(
    `const input = document.querySelector('[data-testid="files-name-input"]')
     input.focus()
     input.select()
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

const textOf = (instance: FlowInstance, hook: string) =>
  instance.exec<string>(
    `const el = document.querySelector('[data-testid="' + arguments[0] + '"]')
     return el ? el.textContent : ''`,
    [hook],
  )

scenario('files-encrypted-device', { timeoutMs: 420_000 }, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-files-encrypted-device' })
  const device = deviceFiles('holzi-encrypted-device-')
  ctx.onTeardown(() => device.remove())
  const png = solidPng(64, 48, [40, 120, 220])
  device.write('notiz.txt', NOTE)
  device.write('foto.png', png)
  device.write('film.mp4', fileFixture('film.mp4'))
  const folderAt = `/device?p=${encodeURIComponent(device.folder)}`

  await runAction(instance, 'wm.app.open', {
    appId: 'system.files',
    at: folderAt,
  })
  await instance.waitForDisplayed('files-entry-notiz.txt')

  // The first encrypted folder of this device: the notice, then the name.
  await contextMenu(instance, 'files-entries')
  await instance.click('files-menu-newEncryptedFolder')
  await instance.waitForDisplayed('files-encrypted-notice')
  await instance.click('files-encrypted-notice-confirm')
  await setName(instance, 'Tresor')
  await instance.waitForDisplayed('files-entry-Tresor')
  const raw = device
    .files()
    .filter((file) => /^[a-z2-7]{26}\.hxef\//.test(file))
  assert.deepEqual(
    raw.map((file) => file.replace(/^[^/]+\//, '')),
    ['h'],
  )
  const dir = (raw[0] ?? '').split('/')[0] ?? ''
  ctx.step('created after the notice: one head in a random directory')

  // A text, an image and a film go in through holzi's clipboard.
  for (const name of ['notiz.txt', 'foto.png', 'film.mp4']) {
    await filesGo(instance, folderAt)
    await instance.waitForDisplayed(`files-entry-${name}`)
    await contextMenu(instance, `files-entry-${name}`)
    await instance.click('files-menu-copy')
    await doubleClick(instance, 'files-entry-Tresor')
    await instance.click('files-action-paste')
    await instance.waitForDisplayed(`files-entry-${name}`, TRANSFER_TIMEOUT_MS)
  }
  ctx.step('a text, an image and a film went in')

  // On the disk: only h, m/… and c/…, no name and no content in the clear.
  const stored = device.files(dir)
  for (const file of stored)
    assert.match(file.slice(dir.length + 1), /^(h|[mc]\/[a-z2-7]{26})$/, file)
  assert.deepEqual(
    device.containing(dir, [...SECRET_NAMES, NOTE]),
    [],
    'a name or the note in the clear',
  )
  assert.equal(stored.length, 7, `h, 3 sidecars, 3 contents: ${stored}`)
  ctx.step('the disk holds nothing in the clear')

  // They open from the encrypted folder.
  await doubleClick(instance, 'files-entry-notiz.txt')
  await instance.waitForDisplayed('files-viewer-text')
  assert.match(await textOf(instance, 'files-viewer-text'), new RegExp(NOTE))
  await instance.click('files-viewer-close')
  await doubleClick(instance, 'files-entry-foto.png')
  await instance.waitForDisplayed('files-viewer-image')
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
  ctx.step('text, image and film open from the encrypted folder')

  // A rename, and a search from the ordinary folder by the decrypted name.
  await contextMenu(instance, 'files-entry-notiz.txt')
  await instance.click('files-menu-rename')
  await setName(instance, 'Belege.txt')
  await instance.waitForDisplayed('files-entry-Belege.txt')
  await filesGo(instance, folderAt)
  await instance.waitForDisplayed('files-entry-Tresor')
  await instance.exec(
    `const input = document.querySelector('[data-testid="files-search"]')
       input.value = 'Belege'
       input.dispatchEvent(new Event('input', { bubbles: true }))
       return true`,
  )
  await instance.waitForDisplayed('files-hit-Belege.txt', 20_000)
  assert.ok(await gone(instance, `files-hit-${dir}`), 'no raw directory hit')
  await instance.click('files-search-clear')
  await instance.waitForDisplayed('files-entry-Tresor')
  ctx.step('a rename and a search by the decrypted names')

  // Another program puts a file into the folder: no entry, a hint, and it stays.
  device.write(`${dir}/notiz (conflicted copy).txt`, 'fremd')
  await doubleClick(instance, 'files-entry-Tresor')
  await instance.waitForDisplayed('files-foreign-hint')
  assert.match(await textOf(instance, 'files-foreign-hint'), /1/)
  assert.ok(await gone(instance, 'files-entry-notiz (conflicted copy).txt'))
  assert.ok(
    device.files(dir).includes(`${dir}/notiz (conflicted copy).txt`),
    'the foreign file stays',
  )
  ctx.step('a foreign file is hinted at and kept')

  // Deleting the folder asks first, names the foreign file and leaves nothing behind.
  await instance.click('files-up')
  await instance.waitForDisplayed('files-entry-Tresor')
  await contextMenu(instance, 'files-entry-Tresor')
  await instance.click('files-menu-delete')
  await instance.waitForDisplayed('files-delete-foreign')
  await instance.click('files-delete-confirm')
  await ctx.waitFor('the folder to go', () =>
    gone(instance, 'files-entry-Tresor'),
  )
  await ctx.waitFor('its directory to go', async () =>
    device.files().every((file) => !file.startsWith(`${dir}/`)),
  )
  assert.deepEqual(device.files(), ['film.mp4', 'foto.png', 'notiz.txt'])
  ctx.step('deleting asks, names the foreign file and removes everything')
})
