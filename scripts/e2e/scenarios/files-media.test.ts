import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { fileFixture } from '../lib/file-fixtures.ts'
import { createAndUnlock, type FlowInstance } from '../lib/flows.ts'
import { doubleClick } from '../lib/passwords.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'

// Spec 044, US2, quickstart §2 (T040): audio and video play from the media server, the video seeks,
// the server answers a range through the web view, a PDF turns to page 2 by key and button, and the URL of a file
// answers 404 once its tab is closed. The fixtures and how they were made:
// src-tauri/tests/fixtures/files/README.md.

/** Plays the viewer's player muted (no user gesture here) and returns its current time once it moves. */
const playing = (instance: FlowInstance, hook: string) =>
  instance.exec<number>(
    `const el = document.querySelector('[data-testid="' + arguments[0] + '"]')
     el.muted = true
     return el.play().then(() => new Promise((resolve) => {
       const check = () => (el.currentTime > 0.3 ? resolve(el.currentTime) : setTimeout(check, 50))
       check()
     }))`,
    [hook],
  )

const mediaUrl = (instance: FlowInstance, hook: string) =>
  instance.exec<string>(
    `return document.querySelector('[data-testid="' + arguments[0] + '"]').src`,
    [hook],
  )

/** Status, `Content-Range` and body length of a fetch from the page. */
const fetchFromPage = (instance: FlowInstance, url: string, range?: string) =>
  instance.exec<{
    status: number
    contentRange: string | null
    length: number
  }>(
    `const headers = arguments[1] ? { Range: arguments[1] } : {}
     return fetch(arguments[0], { headers }).then(async (response) => ({
       status: response.status,
       contentRange: response.headers.get('Content-Range'),
       length: (await response.arrayBuffer()).byteLength,
     }))`,
    [url, range ?? null],
  )

/** The page the PDF canvas last finished, and its width. */
const pdfCanvas = (instance: FlowInstance) =>
  instance.exec<{ page: string | null; width: number }>(
    `const canvas = document.querySelector('[data-testid="files-pdf-canvas"]')
     return { page: canvas?.dataset.renderedPage ?? null, width: canvas?.width ?? 0 }`,
  )

scenario('files-media', { timeoutMs: 240_000 }, async (ctx) => {
  const files = deviceFiles('holzi-media-')
  try {
    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-media' })

    // After the start: on Android it clears the app's data, where the folder lives.
    for (const name of ['film.mp4', 'ton.mp3', 'brief.pdf'])
      files.write(name, fileFixture(name))

    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/device?p=${encodeURIComponent(files.folder)}`,
    })
    await instance.waitForDisplayed('files-entry-film.mp4')

    // Audio plays.
    await doubleClick(instance, 'files-entry-ton.mp3')
    await instance.waitForDisplayed('files-viewer-audio')
    await playing(instance, 'files-viewer-audio')
    ctx.step('the MP3 plays')
    await instance.click('files-viewer-close')

    // Video plays and seeks to the middle.
    await doubleClick(instance, 'files-entry-film.mp4')
    await instance.waitForDisplayed('files-viewer-video')
    await playing(instance, 'files-viewer-video')
    await ctx.waitFor('the video to seek to the middle', () =>
      instance.exec<boolean>(
        `const el = document.querySelector('[data-testid="files-viewer-video"]')
         if (!el.seeking && el.currentTime < 5) el.currentTime = 6
         return !el.seeking && el.currentTime >= 6 && el.readyState >= 2`,
      ),
    )
    ctx.step('the MP4 plays and seeks')

    // The server answers a range through the web view (CSP, CORS and the exposed headers).
    const url = await mediaUrl(instance, 'files-viewer-video')
    const size = fileFixture('film.mp4').byteLength
    assert.deepEqual(await fetchFromPage(instance, url, 'bytes=1000-1999'), {
      status: 206,
      contentRange: `bytes 1000-1999/${size}`,
      length: 1000,
    })
    await instance.click('files-viewer-close')
    // The viewer releases its URL without waiting for the answer.
    await ctx.waitFor(
      'the URL of the closed viewer to end',
      async () => (await fetchFromPage(instance, url)).status === 404,
    )

    // The PDF turns to page 2.
    await doubleClick(instance, 'files-entry-brief.pdf')
    await instance.waitForDisplayed('files-viewer-pdf')
    await ctx.waitFor(
      'page 1 of the PDF',
      async () => (await pdfCanvas(instance)).page === '1',
    )
    // PageDown where the focus is when the PDF opens, then the buttons.
    await instance.exec(
      `document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key: 'PageDown', bubbles: true }))
       return true`,
    )
    await ctx.waitFor(
      'page 2 of the PDF after PageDown',
      async () => (await pdfCanvas(instance)).page === '2',
    )
    await instance.click('files-pdf-previous')
    await ctx.waitFor(
      'page 1 of the PDF again',
      async () => (await pdfCanvas(instance)).page === '1',
    )
    await instance.click('files-pdf-next')
    await ctx.waitFor(
      'page 2 of the PDF',
      async () => (await pdfCanvas(instance)).page === '2',
    )
    const before = (await pdfCanvas(instance)).width
    await instance.click('files-pdf-zoom-in')
    await ctx.waitFor('page 2 larger after zooming', async () => {
      const canvas = await pdfCanvas(instance)
      return canvas.page === '2' && canvas.width > before
    })
    ctx.step('the PDF turns and zooms')

    // Closing the tab ends the URLs it still holds.
    await instance.click('files-viewer-close')
    await doubleClick(instance, 'files-entry-film.mp4')
    await instance.waitForDisplayed('files-viewer-video')
    const held = await mediaUrl(instance, 'files-viewer-video')
    assert.equal((await fetchFromPage(instance, held, 'bytes=0-0')).status, 206)
    const closed = await runAction(instance, 'wm.tab.close', {})
    assert.ok(closed.ok, `wm.tab.close: ${JSON.stringify(closed)}`)
    await ctx.waitFor(
      'the URL to end with its tab',
      async () => (await fetchFromPage(instance, held)).status === 404,
    )
  } finally {
    files.remove()
  }
})
