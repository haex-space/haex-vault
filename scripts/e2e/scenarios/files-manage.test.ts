import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { createAndUnlock, unwrap, type FlowInstance } from '../lib/flows.ts'
import { contextMenu, doubleClick, dragTo } from '../lib/passwords.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'

// Spec 044, US3, quickstart §3 (T052): a new folder, a rename that clashes and one that works, a
// move by dragging inside holzi, a copy whose name is taken (keep both), a copy cancelled at its
// conflict (nothing half-written stays), a folder pasted into itself (refused), and a delete.

/** Every name in a folder of the device, hidden ones too (part files start with a dot). */
async function allNames(instance: FlowInstance, path: string) {
  const entries = unwrap<{ name: string }[]>(
    'files_list',
    await instance.invoke('files_list', {
      source: { kind: 'device' },
      path,
    }),
  )
  return entries.map((entry) => entry.name).sort()
}

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

const gone = (instance: FlowInstance, hook: string) =>
  instance.exec<boolean>(
    `return !document.querySelector('[data-testid="' + arguments[0] + '"]')`,
    [hook],
  )

scenario('files-manage', { timeoutMs: 240_000 }, async (ctx) => {
  const files = deviceFiles('holzi-manage-')
  try {
    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-manage' })

    // After the start: on Android it clears the app's data, where the folder lives.
    files.write('a.txt', 'a')
    files.write('b.txt', 'alt')
    unwrap(
      'files_create_folder',
      await instance.invoke('files_create_folder', {
        source: { kind: 'device' },
        path: files.folder,
        name: 'ordner',
      }),
    )
    files.write('ordner/b.txt', 'neu')
    files.write('ordner/x.txt', 'x')
    const root = files.folder
    const inner = files.path('ordner')

    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/device?p=${encodeURIComponent(root)}`,
    })
    await instance.waitForDisplayed('files-entry-a.txt')

    // A new folder from the menu of the empty area.
    await contextMenu(instance, 'files-entries')
    await instance.click('files-menu-newFolder')
    await setName(instance, 'Neu')
    await instance.waitForDisplayed('files-entry-Neu')
    ctx.step('a new folder')

    // A rename onto a taken name is refused; another name works.
    await contextMenu(instance, 'files-entry-a.txt')
    await instance.click('files-menu-rename')
    await setName(instance, 'b.txt')
    await instance.waitForDisplayed('files-name-error')
    await setName(instance, 'c.txt')
    await instance.waitForDisplayed('files-entry-c.txt')
    assert.ok(await gone(instance, 'files-entry-a.txt'))
    ctx.step('rename: a taken name refused, a free one taken')

    // Dragging onto a folder inside holzi moves.
    await dragTo(instance, 'files-entry-c.txt', 'files-entry-Neu')
    await ctx.waitFor('c.txt to leave the folder', () =>
      gone(instance, 'files-entry-c.txt'),
    )
    assert.deepEqual(await allNames(instance, files.path('Neu')), ['c.txt'])
    ctx.step('moved by dragging')

    // A copy whose name is taken: keep both.
    await doubleClick(instance, 'files-entry-ordner')
    await instance.waitForDisplayed('files-entry-x.txt')
    await contextMenu(instance, 'files-entry-b.txt')
    await instance.click('files-menu-copy')
    await instance.click('files-up')
    await instance.waitForDisplayed('files-entry-ordner')
    await instance.click('files-action-paste')
    await instance.waitForDisplayed('files-conflict')
    await instance.click('files-conflict-keep-both')
    await instance.waitForDisplayed('files-entry-b (2).txt')
    assert.equal(files.read('b.txt'), 'alt')
    assert.equal(files.read('b (2).txt'), 'neu')
    ctx.step('a taken name: keep both')

    // Cancelled at its conflict: nothing new, no part file.
    // Paste clears the clipboard now, so copy the source again for this second attempt.
    await contextMenu(instance, 'files-entry-b.txt')
    await instance.click('files-menu-copy')
    await instance.click('files-action-paste')
    await instance.waitForDisplayed('files-conflict')
    await instance.click('files-conflict-cancel')
    await ctx.waitFor('the transfer to end', () =>
      gone(instance, 'files-transfers'),
    )
    assert.deepEqual(await allNames(instance, root), [
      'Neu',
      'b (2).txt',
      'b.txt',
      'ordner',
    ])
    ctx.step('a cancelled copy leaves nothing behind')

    // A folder pasted into itself is refused before anything starts.
    await contextMenu(instance, 'files-entry-ordner')
    await instance.click('files-menu-copy')
    await doubleClick(instance, 'files-entry-ordner')
    await instance.waitForDisplayed('files-entry-x.txt')
    await instance.click('files-action-paste')
    await ctx.waitFor('the refusal', () =>
      instance.exec<boolean>(
        `return Boolean(document.querySelector('[data-sonner-toast]'))`,
      ),
    )
    assert.deepEqual(await allNames(instance, inner), ['b.txt', 'x.txt'])
    ctx.step('a folder into itself refused')

    // Delete: into the trash on desktops; where it is for good, holzi asks first.
    await instance.click('files-up')
    await instance.waitForDisplayed('files-entry-b (2).txt')
    await contextMenu(instance, 'files-entry-b (2).txt')
    await instance.click('files-menu-delete')
    await ctx.waitFor('b (2).txt to go', async () => {
      const asks = await instance.exec<boolean>(
        `return Boolean(document.querySelector('[data-testid="files-delete-confirm"]'))`,
      )
      if (asks) await instance.click('files-delete-confirm')
      return gone(instance, 'files-entry-b (2).txt')
    })
    assert.deepEqual(await allNames(instance, root), ['Neu', 'b.txt', 'ordner'])
    ctx.step('deleted')
  } finally {
    files.remove()
  }
})
