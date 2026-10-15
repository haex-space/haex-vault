import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import type { FlowInstance } from '../lib/flows.ts'
import { scenario } from '../lib/scenario.ts'
import { openSettings, runAction, waitForLocation } from '../lib/settings.ts'

// Spec 053, quickstart §2 steps 1 to 3 and 6 (US1): the settings list Lucide, Phosphor and
// Material with a preview; choosing Phosphor shows the images of the set in the open folder as
// `img` elements (never `svg` elements in the DOM) at once and says "Gespeichert." without a save
// button; the images differ between the light and the dark scheme; the choice survives a restart.
// The pictures of the states (no access, locked, damaged) are checked in Rust, `bundled_tests.rs`.

const SET = (id: string) => `settings-icon-set-option-${id}`

/** What an entry's picture is: the tag of the element in its row and whether an image loaded. */
const pictureOf = (instance: FlowInstance, name: string) =>
  instance.exec<{ tag: string; src: string; loaded: boolean; svgs: number }>(
    `const row = document.querySelector('[data-testid="files-entry-' + arguments[0] + '"]')
     const img = row && row.querySelector('img')
     const first = row && row.querySelector('img, svg')
     return {
       tag: first ? first.tagName.toLowerCase() : '',
       src: img ? img.getAttribute('src') || '' : '',
       loaded: !!img && img.complete && img.naturalWidth > 0,
       svgs: row ? row.querySelectorAll('svg').length : -1,
     }`,
    [name],
  )

const openFolder = (instance: FlowInstance, folder: string) =>
  runAction(instance, 'wm.app.open', {
    appId: 'system.files',
    at: `/device?p=${encodeURIComponent(folder)}`,
  })

scenario('files-icon-sets', { timeoutMs: 300_000 }, async (ctx) => {
  const files = deviceFiles('holzi-icons-')
  try {
    const group = await ctx.group({ users: { anna: ['laptop'] } })
    const device = group.device('anna/laptop')
    let page = device.page

    files.write('bericht.pdf', 'pdf')
    files.write('main.rs', 'fn main() {}')
    files.write('daten.csv', 'a;b')
    files.write('unbekannt.zzz', 'x')
    // 1. The group lists the three sets, Lucide chosen, each with a preview.
    await openSettings(page)
    await page.click('settings-category-general')
    await page.click('settings-row-general.appearance')
    await waitForLocation(page, 'general.appearance')
    await page.waitForDisplayed('settings-icon-set')
    for (const id of ['builtin:lucide', 'builtin:phosphor', 'builtin:material'])
      await page.waitForDisplayed(SET(id))
    assert.equal(
      await page.exec<string | null>(
        `const el = document.querySelector('[data-testid="${SET('builtin:lucide')}"]')
         return el && el.getAttribute('data-state')`,
      ),
      'checked',
      'Lucide is chosen at first',
    )
    ctx.step('the group lists Lucide, Phosphor and Material')

    // 2. Phosphor at once: img elements in the list, a saved note, no save button.
    await page.click(SET('builtin:phosphor'))
    await ctx.waitFor(
      'the saved note',
      async () =>
        await page.exec<boolean>(
          `return document.body.textContent.includes('Gespeichert.')`,
        ),
      { fixed: true, timeoutMs: 10_000 },
    )
    assert.equal(
      await page.exec<number>(
        `return document.querySelectorAll('[data-testid="settings-icon-set"] button[type="submit"]').length`,
      ),
      0,
      'there is no save button',
    )
    await openFolder(page, files.folder)
    await page.waitForDisplayed('files-entry-bericht.pdf')
    await ctx.waitFor(
      'the Phosphor images in the listing',
      async () => {
        const pdf = await pictureOf(page, 'bericht.pdf')
        const rust = await pictureOf(page, 'main.rs')
        return pdf.loaded && rust.loaded
      },
      { fixed: true, timeoutMs: 5_000 },
    )
    for (const name of [
      'bericht.pdf',
      'main.rs',
      'daten.csv',
      'unbekannt.zzz',
    ]) {
      const picture = await pictureOf(page, name)
      assert.equal(picture.tag, 'img', `${name} is an image`)
      assert.equal(picture.svgs, 0, `${name} has no svg element`)
      assert.match(picture.src, /^blob:/)
    }
    ctx.step('Phosphor shows img elements at once')

    // The two schemes give two images.
    await runAction(page, 'settings.appearance.setColorScheme', {
      scheme: 'dark',
    })
    await ctx.waitFor(
      'the dark image',
      async () => (await pictureOf(page, 'bericht.pdf')).loaded,
      { fixed: true, timeoutMs: 5_000 },
    )
    const dark = (await pictureOf(page, 'bericht.pdf')).src
    await runAction(page, 'settings.appearance.setColorScheme', {
      scheme: 'light',
    })
    await ctx.waitFor(
      'the light image',
      async () => {
        const light = await pictureOf(page, 'bericht.pdf')
        return light.loaded && light.src !== dark
      },
      { fixed: true, timeoutMs: 5_000 },
    )
    ctx.step('light and dark scheme give different images')

    // 3. The choice survives a restart.
    await device.restart()
    page = device.page
    await openFolder(page, files.folder)
    await page.waitForDisplayed('files-entry-bericht.pdf')
    await ctx.waitFor(
      'Phosphor after the restart',
      async () => (await pictureOf(page, 'bericht.pdf')).tag === 'img',
      { fixed: true, timeoutMs: 30_000 },
    )
    ctx.step('the choice survives a restart')
  } finally {
    files.remove()
  }
})
