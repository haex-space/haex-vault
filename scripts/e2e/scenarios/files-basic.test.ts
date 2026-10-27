import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { createAndUnlock, type FlowInstance } from '../lib/flows.ts'
import { solidPng } from '../lib/png.ts'
import { doubleClick } from '../lib/passwords.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'

// Spec 044, US1, quickstart §1 (T035 part 1): the file browser lists a folder, opens a text and an
// image, switches to the grid with a thumbnail, shows a file another program creates, goes up one
// folder and back into the folder through the list. A long folder keeps its scroll position when it
// loads again.

const textOf = (instance: FlowInstance, hook: string) =>
  instance.exec<string>(
    `const el = document.querySelector('[data-testid="' + arguments[0] + '"]')
     return el ? el.textContent : ''`,
    [hook],
  )

const entryNames = (instance: FlowInstance) =>
  instance.exec<string[]>(
    `return [...document.querySelectorAll('[data-testid^="files-entry-"]')]
       .map((el) => el.getAttribute('data-testid').slice('files-entry-'.length))`,
  )

const scrollTop = (instance: FlowInstance) =>
  instance.exec<number>(
    `const el = document.querySelector('[data-testid="files-entries"]')
     return el ? el.scrollTop : -1`,
  )
/** Scrolls the virtual list until the entry `name` is rendered (a busy parent folder, like a shared
 * temporary folder, can push it below the first screen). */
const scrollTo = (instance: FlowInstance, name: string) =>
  instance.exec<boolean>(
    `const list = document.querySelector('[data-testid="files-entries"]')
     const hook = '[data-testid="files-entry-' + arguments[0] + '"]'
     return new Promise((resolve) => {
       const step = () => {
         if (document.querySelector(hook)) return resolve(true)
         if (!list || list.scrollTop + list.clientHeight >= list.scrollHeight) return resolve(false)
         list.scrollTop += list.clientHeight
         requestAnimationFrame(() => requestAnimationFrame(step))
       }
       if (!list) return resolve(false)
       // The folder view keeps its scroll offset while the new parent folder is loading. Start at
       // the top so the search works in either direction, then let the virtual list render before
       // measuring its bounds.
       list.scrollTop = 0
       requestAnimationFrame(() => requestAnimationFrame(step))
     })`,
    [name],
  )

scenario('files-basic', { timeoutMs: 180_000 }, async (ctx) => {
  const files = deviceFiles('holzi-files-')
  try {
    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-basic' })

    // After the start: on Android it clears the app's data, where the folder lives.
    files.write('notiz.txt', 'Hallo aus der Datei')
    files.write('foto.png', solidPng(64, 48, [220, 40, 40]))
    const folderName = files.folder.split('/').at(-1) ?? ''
    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/device?p=${encodeURIComponent(files.folder)}`,
    })

    await instance.waitForDisplayed('files-entry-notiz.txt')
    assert.deepEqual(await entryNames(instance), ['foto.png', 'notiz.txt'])

    // A text opens in the viewer.
    await doubleClick(instance, 'files-entry-notiz.txt')
    await instance.waitForDisplayed('files-viewer-text')
    assert.match(
      await textOf(instance, 'files-viewer-text'),
      /Hallo aus der Datei/,
    )
    await instance.click('files-viewer-close')

    // An image opens too.
    await doubleClick(instance, 'files-entry-foto.png')
    await instance.waitForDisplayed('files-viewer-image')
    await instance.click('files-viewer-close')

    // The grid shows a thumbnail for the image.
    await instance.click('files-view-toggle')
    await ctx.waitFor('the thumbnail of foto.png', () =>
      instance.exec<boolean>(
        `const img = document.querySelector('[data-testid="files-entry-foto.png"] img')
         return Boolean(img && img.complete && img.naturalWidth > 0)`,
      ),
    )
    await instance.click('files-view-toggle')

    // A file another program creates appears without reloading.
    files.write('neu.txt', 'neu')
    await instance.waitForDisplayed('files-entry-neu.txt', 15_000)

    // A long folder keeps where it was scrolled to when it loads again; the virtual list mounts
    // only the rows on screen, so the new last entry shows only if the list stayed at the end.
    for (let i = 0; i < 80; i++) {
      files.write(`zeile-${String(i).padStart(3, '0')}.txt`, `${i}`)
    }
    await instance.waitForDisplayed('files-entry-zeile-000.txt', 15_000)
    // The list may have loaded while the files were still being written (each write is a few adb
    // calls on the phone): scroll to the end until the last one is there.
    await ctx.waitFor(
      'the last row at the end of the list',
      () =>
        instance.exec<boolean>(
          `const list = document.querySelector('[data-testid="files-entries"]')
           list.scrollTop = 1e6
           const row = document.querySelector('[data-testid="files-entry-zeile-079.txt"]')
           return Boolean(row && row.getClientRects().length)`,
        ),
      { timeoutMs: 15_000 },
    )
    files.write('zeile-999.txt', 'ende')
    await ctx.waitFor(
      'the new last row at the end of the list',
      () =>
        instance.exec<boolean>(
          `const list = document.querySelector('[data-testid="files-entries"]')
           if (!list) return false
           list.scrollTop = 1e6
           const row = document.querySelector('[data-testid="files-entry-zeile-999.txt"]')
           return Boolean(row && row.getClientRects().length && list.scrollTop > 0)`,
        ),
      { timeoutMs: 30_000 },
    )

    // Up one folder and back into the folder through the list.
    await instance.click('files-up')
    await ctx.waitFor('the folder in its parent', () =>
      scrollTo(instance, folderName),
    )
    await doubleClick(instance, `files-entry-${folderName}`)
    await instance.waitForDisplayed('files-entry-notiz.txt')
    // A folder opened again starts at the top.
    assert.equal(await scrollTop(instance), 0)
  } finally {
    files.remove()
  }
})
