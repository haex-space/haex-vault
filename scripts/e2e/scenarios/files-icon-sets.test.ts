import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { unwrap, type FlowInstance } from '../lib/flows.ts'
import { zipOf } from '../lib/zip.ts'
import { scenario } from '../lib/scenario.ts'
import { openSettings, runAction, waitForLocation } from '../lib/settings.ts'

// Spec 053, quickstart §2 steps 1 to 5 (US1, US3): the settings list Lucide, Phosphor and
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
    // 4. An own set from an archive: the report, the import, the set in the list; a script in
    // its SVG has no effect and the stored image is the clean one.
    const evil =
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" onload="alert(1)">' +
      '<script>alert(2)</script><path d="M2 2h12v12H2z" fill="#c00"/></svg>'
    files.write(
      'demo-set.zip',
      zipOf([
        {
          name: 'icon-theme.json',
          data: JSON.stringify({
            iconDefinitions: {
              file: { iconPath: './icons/file.svg' },
              evil: { iconPath: './icons/evil.svg' },
            },
            file: 'file',
            fileExtensions: { evil: 'evil' },
          }),
        },
        { name: 'icons/file.svg', data: evil.replace('onload="alert(1)"', '') },
        { name: 'icons/evil.svg', data: evil },
        { name: 'LICENSE', data: 'MIT' },
      ]),
    )
    files.write('probe.evil', 'x')
    const source = { kind: 'file', file: files.path('demo-set.zip') }
    const report = unwrap<{
      name: string
      imageCount: number
      skipped: unknown[]
      sameName: string | null
    }>('preview', await page.invoke('file_icon_set_import_preview', { source }))
    assert.equal(report.name, 'demo-set')
    assert.equal(report.imageCount, 2)
    assert.deepEqual(report.skipped, [])
    assert.equal(report.sameName, null)
    const added = unwrap<{ ref: { id: string } }>(
      'import',
      await page.invoke('file_icon_set_import', { source, replace: null }),
    )
    const custom = `custom:${added.ref.id}`
    await openSettings(page)
    await page.click('settings-category-general')
    await page.click('settings-row-general.appearance')
    await waitForLocation(page, 'general.appearance')
    await page.waitForDisplayed(SET(custom))
    await page.click(SET(custom))
    await openFolder(page, files.folder)
    await page.waitForDisplayed('files-entry-probe.evil')
    await ctx.waitFor(
      'the image of the own set',
      async () => (await pictureOf(page, 'probe.evil')).loaded,
      { fixed: true, timeoutMs: 10_000 },
    )
    // The bytes the window gets for the image (a `blob:` URL cannot be fetched under the CSP).
    const stored = await page.exec<string>(
      `return window.__TAURI_INTERNALS__
         .invoke('files_icon_image', { set: { kind: 'custom', id: arguments[0] }, id: 'icons/evil.svg' })
         .then((bytes) => new TextDecoder().decode(bytes))`,
      [added.ref.id],
    )
    assert.doesNotMatch(stored, /script|onload|alert/)
    assert.match(stored, /<path/)
    ctx.step('an own set is imported, shown and clean')

    // 5. Removing it asks first; the set is gone and the choice falls to a built-in one.
    await page.waitForDisplayed(SET(custom))
    await page.exec(
      `document.querySelector('[data-testid="${SET(custom)}"]')
         .closest('li').querySelector('[data-testid="settings-icon-set-remove"]').click()`,
    )
    await page.click('settings-icon-set-remove-confirm')
    await ctx.waitFor(
      'the set to be gone',
      async () =>
        await page.exec<boolean>(
          `return !document.querySelector('[data-testid="${SET(custom)}"]')`,
        ),
      { fixed: true, timeoutMs: 10_000 },
    )
    const checked = await page.exec<string | null>(
      `const el = document.querySelector('[data-testid^="settings-icon-set-option-"][data-state="checked"]')
       return el && el.getAttribute('data-testid')`,
    )
    assert.match(checked ?? '', /settings-icon-set-option-builtin:/)
    ctx.step('removing asks first and falls back to a built-in set')
  } finally {
    files.remove()
  }
})
