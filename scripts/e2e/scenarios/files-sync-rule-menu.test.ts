import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { createAndUnlock, unwrap, type FlowInstance } from '../lib/flows.ts'
import { contextMenu } from '../lib/passwords.ts'
import { scenario, type ScenarioContext } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'

// Spec 073: "Sync-Regel erstellen…" in the context menus of the file browser. A folder entry, a file entry
// and the open folder's empty area each open the sync rule dialog with that place as the source, the name
// following the source and no target picked; a file shows no delete mode (spec 074). Cancelling creates no
// rule. The target is not picked here: the roots of the folder picker differ per platform and a temporary
// folder is not among them; saving, the checks and the confirmation are the dialog's own (spec 061) and
// checked by hand (quickstart US1.3).

const FILE = 'Haushalt.ods'

const gone = (instance: FlowInstance, hook: string) =>
  instance.exec<boolean>(
    `return !document.querySelector('[data-testid="' + arguments[0] + '"]')`,
    [hook],
  )

const textOf = (instance: FlowInstance, hook: string) =>
  instance.exec<string>(
    `return document.querySelector('[data-testid="' + arguments[0] + '"]').textContent.trim()`,
    [hook],
  )

const nameValue = (instance: FlowInstance) =>
  instance.exec<string>(
    `const el = document.querySelector('[data-testid="files-sync-rule-name"]')
     return (el.matches('input') ? el : el.querySelector('input')).value`,
  )

/** Escape cancels the dialog; the next step waits until it is gone. */
async function closeModal(instance: FlowInstance, ctx: ScenarioContext) {
  await instance.exec(
    `document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', keyCode: 27, bubbles: true }))`,
  )
  await ctx.waitFor(
    'the dialog is closed',
    () => gone(instance, 'files-sync-rule-form'),
    { timeoutMs: 10_000 },
  )
}

const ruleCount = async (instance: FlowInstance) =>
  unwrap<unknown[]>('sync_rules_list', await instance.invoke('sync_rules_list'))
    .length

scenario('files-sync-rule-menu', { timeoutMs: 180_000 }, async (ctx) => {
  const files = deviceFiles('holzi-rule-menu-')
  try {
    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-rule-menu' })

    // After the start: on Android it clears the app's data, where the folder lives.
    files.ensureFolder()
    unwrap(
      'files_create_folder',
      await instance.invoke('files_create_folder', {
        source: { kind: 'device' },
        path: files.folder,
        name: 'alpha',
      }),
    )
    files.write('alpha/in-alpha.txt', 'a')
    files.write(FILE, 'eins')

    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/device?p=${encodeURIComponent(files.folder)}`,
    })
    await instance.waitForDisplayed('files-entry-alpha')

    // A folder entry: the dialog opens with the folder as the source and its name, no target.
    await contextMenu(instance, 'files-entry-alpha')
    await instance.click('files-menu-syncRule')
    await instance.waitForDisplayed('files-sync-rule-form')
    await instance.waitForDisplayed('files-picker-source-chosen')
    assert.match(await textOf(instance, 'files-picker-source-path'), /alpha$/)
    assert.equal(await nameValue(instance), 'alpha')
    assert.equal(
      await gone(instance, 'files-picker-target-chosen'),
      true,
      'no target is prefilled',
    )
    await instance.waitForDisplayed('files-sync-rule-delete-mode')
    // Cancelled: no rule.
    await closeModal(instance, ctx)
    assert.equal(await ruleCount(instance), 0)
    ctx.step('a folder entry opens the dialog')

    // A file entry: the file is the source, and a file rule has no delete mode.
    await contextMenu(instance, `files-entry-${FILE}`)
    await instance.click('files-menu-syncRule')
    await instance.waitForDisplayed('files-picker-source-chosen')
    assert.match(
      await textOf(instance, 'files-picker-source-path'),
      new RegExp(`${FILE.replace('.', '\\.')}$`),
    )
    assert.equal(await gone(instance, 'files-sync-rule-delete-mode'), true)
    await closeModal(instance, ctx)
    assert.equal(await ruleCount(instance), 0)
    ctx.step('a file entry opens the dialog as a file rule')

    // The open folder: the empty area's menu.
    await contextMenu(instance, 'files-entries')
    await instance.click('files-menu-syncRule')
    await instance.waitForDisplayed('files-picker-source-chosen')
    assert.equal(
      await textOf(instance, 'files-picker-source-path'),
      files.folder,
    )
    await closeModal(instance, ctx)
    assert.equal(await ruleCount(instance), 0)
    ctx.step('the open folder opens the dialog')
  } finally {
    files.remove()
  }
})
