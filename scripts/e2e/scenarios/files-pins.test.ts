import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { createAndUnlock, unwrap, type FlowInstance } from '../lib/flows.ts'
import { contextMenu, dragTo } from '../lib/passwords.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'

// Spec 071: folders added to "Orte" in the sidebar and "Meine Dateifreigaben" as a list in the panel.
// A folder is added from its context menu, opens from the sidebar, and goes again by its own context
// menu there; a folder dragged onto "Orte" is added, an added one dragged onto the strip is removed,
// and the folder stays in both cases. The list of shares shows in the panel, with no dialog.

const gone = (instance: FlowInstance, hook: string) =>
  instance.exec<boolean>(
    `return !document.querySelector('[data-testid="' + arguments[0] + '"]')`,
    [hook],
  )

async function folderNames(instance: FlowInstance, path: string) {
  const entries = unwrap<{ name: string }[]>(
    'files_list',
    await instance.invoke('files_list', { source: { kind: 'device' }, path }),
  )
  return entries.map((entry) => entry.name).sort()
}

scenario('files-pins', { timeoutMs: 240_000 }, async (ctx) => {
  const files = deviceFiles('holzi-pins-')
  try {
    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-pins' })

    // After the start: on Android it clears the app's data, where the folder lives.
    files.ensureFolder()
    for (const name of ['alpha', 'beta']) {
      unwrap(
        'files_create_folder',
        await instance.invoke('files_create_folder', {
          source: { kind: 'device' },
          path: files.folder,
          name,
        }),
      )
    }
    files.write('alpha/in-alpha.txt', 'a')

    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/device?p=${encodeURIComponent(files.folder)}`,
    })
    await instance.waitForDisplayed('files-entry-alpha')
    await instance.waitForDisplayed('files-places')

    // Pinned from the context menu, the pin opens its folder from the sidebar.
    await contextMenu(instance, 'files-entry-alpha')
    await instance.click('files-menu-pin')
    await instance.waitForDisplayed('files-pin-alpha')
    await contextMenu(instance, 'files-entry-alpha')
    await instance.waitForDisplayed('files-menu-unpin')
    await instance.click('files-menu-unpin')
    await ctx.waitFor('the pin to go', () => gone(instance, 'files-pin-alpha'))
    await contextMenu(instance, 'files-entry-alpha')
    await instance.click('files-menu-pin')
    await instance.waitForDisplayed('files-pin-alpha')
    await instance.click('files-pin-alpha')
    await instance.waitForDisplayed('files-entry-in-alpha.txt')
    ctx.step('pinned from the menu, opened from the sidebar')

    // The pin goes by its own context menu; the folder stays.
    await instance.click('files-up')
    await instance.waitForDisplayed('files-entry-beta')
    await contextMenu(instance, 'files-pin-alpha')
    await instance.click('files-menu-unpin')
    await ctx.waitFor('the pin to go', () => gone(instance, 'files-pin-alpha'))
    assert.deepEqual(await folderNames(instance, files.folder), [
      'alpha',
      'beta',
    ])
    ctx.step('unpinned from the sidebar menu')

    // Dropping a folder on "Orte" adds it; dragging the pin onto the strip removes it.
    await dragTo(instance, 'files-entry-beta', 'files-places')
    await instance.waitForDisplayed('files-pin-beta')
    await dragTo(instance, 'files-pin-beta', 'files-pins-unpin-strip')
    await ctx.waitFor('the pin to go', () => gone(instance, 'files-pin-beta'))
    await ctx.waitFor('the strip to go', () =>
      gone(instance, 'files-pins-unpin-strip'),
    )
    assert.deepEqual(await folderNames(instance, files.folder), [
      'alpha',
      'beta',
    ])
    ctx.step('pinned and unpinned by dragging, the folders stay')

    // "Meine Dateifreigaben" is a list in the panel, not a dialog.
    await instance.click('files-my-shares')
    await instance.waitForDisplayed('files-shares-empty')
    assert.ok(
      await instance.exec<boolean>(
        `return !document.querySelector('[role="dialog"]')`,
      ),
      'no dialog opens',
    )
    ctx.step('the shares show as a list in the panel')
  } finally {
    files.remove()
  }
})
