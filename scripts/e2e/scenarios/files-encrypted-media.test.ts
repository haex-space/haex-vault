import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { fileFixture } from '../lib/file-fixtures.ts'
import { createAndUnlock, type FlowInstance } from '../lib/flows.ts'
import { contextMenu } from '../lib/passwords.ts'
import { startRustfs } from '../lib/rustfs.ts'
import { scenario } from '../lib/scenario.ts'
import { connectStorage, filesGo } from '../lib/storage-setup.ts'
import { runAction } from '../lib/settings.ts'

// Spec 048, US4, quickstart §5 (T040): video and audio from an encrypted folder in a storage of
// RustFS. The MP3 and MP4 fixtures of spec 044 go in from the device, the MP3 plays, the MP4 plays
// and seeks to the middle, and a range fetched from the page through the viewer's media URL is a
// 206 with the fixture's bytes at that range, across a block border (64 KiB). Kept apart from
// `files-encrypted` so the two can change independently. Needs Docker for RustFS; skipped
// without it.

const BUCKET = 'holzi-encrypted-media-e2e'
const MEDIA = ['ton.mp3', 'film.mp4']

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

/** Status, `Content-Range` and body of a range fetched from the page. */
const fetchRange = (instance: FlowInstance, url: string, range: string) =>
  instance.exec<{
    status: number
    contentRange: string | null
    body: number[]
  }>(
    `return fetch(arguments[0], { headers: { Range: arguments[1] } }).then(async (response) => ({
       status: response.status,
       contentRange: response.headers.get('Content-Range'),
       body: Array.from(new Uint8Array(await response.arrayBuffer())),
     }))`,
    [url, range],
  )

scenario(
  'files-encrypted-media',
  { timeoutMs: 300_000, needs: { container: true } },
  async (ctx) => {
    const rustfs = await startRustfs([BUCKET])
    ctx.onTeardown(() => rustfs.stop())
    ctx.step('RustFS')

    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-encrypted-media' })
    // After the start: on Android it clears the app's data, where the folder lives.
    const device = deviceFiles('holzi-encrypted-media-')
    ctx.onTeardown(() => device.remove())
    for (const name of MEDIA) device.write(name, fileFixture(name))
    const storageId = await connectStorage(instance, rustfs, BUCKET, 'Medien')

    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/storage/${storageId}?p=${encodeURIComponent('/')}`,
    })
    await instance.waitForDisplayed('files-empty')
    await contextMenu(instance, 'files-empty')
    await instance.click('files-menu-newEncryptedFolder')
    await instance.waitForDisplayed('files-encrypted-notice')
    await instance.click('files-encrypted-notice-confirm')
    await setName(instance, 'Filme')
    await instance.waitForDisplayed('files-entry-Filme')

    // In from the device, one by one through holzi's clipboard.
    for (const name of MEDIA) {
      await filesGo(instance, `/device?p=${encodeURIComponent(device.folder)}`)
      await instance.waitForDisplayed(`files-entry-${name}`)
      await contextMenu(instance, `files-entry-${name}`)
      await instance.click('files-menu-copy')
      await instance.click('files-storage-Medien')
      await instance.waitForDisplayed('files-entry-Filme')
      await instance.click('files-entry-Filme')
      await instance.click('files-action-paste')
      await instance.waitForDisplayed(`files-entry-${name}`)
    }
    ctx.step('MP3 and MP4 went into the encrypted folder')

    await instance.click('files-entry-ton.mp3')
    await instance.waitForDisplayed('files-viewer-audio')
    await ctx.waitFor('the MP3 to play', () =>
      instance.exec<boolean>(
        `const el = document.querySelector('[data-testid="files-viewer-audio"]')
         el.muted = true
         if (el.paused) void el.play().catch(() => {})
         return el.currentTime > 0.3`,
      ),
    )
    await instance.click('files-viewer-close')
    ctx.step('the MP3 plays')

    await instance.click('files-entry-film.mp4')
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
    ctx.step('the MP4 plays and seeks to the middle')

    // A range across the border of the first two blocks, decrypted by holzi.
    const film = fileFixture('film.mp4')
    const url = await instance.exec<string>(
      `return document.querySelector('[data-testid="files-viewer-video"]').src`,
    )
    const answer = await fetchRange(instance, url, 'bytes=65000-66999')
    assert.equal(answer.status, 206)
    assert.equal(answer.contentRange, `bytes 65000-66999/${film.byteLength}`)
    assert.ok(
      Buffer.from(answer.body).equals(film.subarray(65000, 67000)),
      'the bytes of the fixture at that range',
    )
    await instance.click('files-viewer-close')
    ctx.step('a range from the page is a 206 with the right bytes')
  },
)
