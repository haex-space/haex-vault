import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { entryIn, inPanel } from '../lib/files-split.ts'
import { createAndUnlock, unwrap, type FlowInstance } from '../lib/flows.ts'
import { contextMenu, doubleClick } from '../lib/passwords.ts'
import { scenario, type ScenarioContext } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'

// Spec 073: "Sync-Regel erstellen …" in the context menus of the file browser. A folder entry, a file entry
// and the open folder's empty area each open the sync rule dialog with that place as the source, the name
// following the source and no target picked; a file shows no delete mode (spec 074). Cancelling creates no
// rule. Saving uses a fresh target below one of the device's known places, so the picker can reach it on
// every platform. The persisted rule, the list refresh, focus and the right split panel are checked too.

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
    await instance.exec(
      `document.querySelector('[data-testid="files-entry-' + arguments[0] + '"]').focus()`,
      [FILE],
    )
    await contextMenu(instance, `files-entry-${FILE}`)
    await instance.click('files-menu-syncRule')
    await instance.waitForDisplayed('files-picker-source-chosen')
    assert.match(
      await textOf(instance, 'files-picker-source-path'),
      new RegExp(`${FILE.replace('.', '\\.')}$`),
    )
    assert.equal(await gone(instance, 'files-sync-rule-delete-mode'), true)
    await closeModal(instance, ctx)
    await ctx.waitFor('focus returns to the file, not the first row', () =>
      instance.exec<boolean>(
        `return document.activeElement?.closest('[data-path]')?.getAttribute('data-testid') === arguments[0]`,
        [`files-entry-${FILE}`],
      ),
    )
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
    await ctx.waitFor('focus returns to the empty area', () =>
      instance.exec<boolean>(
        `return document.activeElement?.matches('[data-files-list]') ?? false`,
      ),
    )
    assert.equal(await ruleCount(instance), 0)
    ctx.step('the open folder opens the dialog')

    // Multiple selections must not silently use their first entry.
    await instance.exec(
      `for (const name of ['alpha', arguments[0]]) {
         document.querySelector('[data-testid="files-entry-' + name + '"]')
           .dispatchEvent(new MouseEvent('click', { bubbles: true, ctrlKey: true }))
       }`,
      [FILE],
    )
    await contextMenu(instance, 'files-entry-alpha')
    await instance.waitForDisplayed('files-context-menu')
    assert.equal(await gone(instance, 'files-menu-syncRule'), true)
    await instance.exec(
      `document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`,
    )
    await instance.click('files-entries')

    const sources = unwrap<{ known: { name: string; path: string }[] }>(
      'files_sources',
      await instance.invoke('files_sources', {}),
    )
    const root =
      sources.known.find((place) => place.name === 'downloads') ??
      sources.known[0]
    assert.ok(root, 'the device has a known place for the target picker')
    for (const [entry, kind, targetName] of [
      ['alpha', 'folder', 'holzi-menu-folder-target'],
      [FILE, 'file', 'holzi-menu-file-target'],
    ] as const) {
      const target = unwrap<{ path: string }>(
        'files_create_folder',
        await instance.invoke('files_create_folder', {
          source: { kind: 'device' },
          path: root.path,
          name: targetName,
        }),
      )
      await contextMenu(instance, `files-entry-${entry}`)
      await instance.click('files-menu-syncRule')
      await instance.waitForDisplayed('files-picker-source-chosen')
      await instance.click('files-picker-target-root')
      await instance.click('[role="option"]')
      await instance.click(`files-picker-target-folder-${root.name}`)
      await instance.click(`files-picker-target-folder-${targetName}`)
      await instance.click('files-picker-target-choose')
      await instance.click('files-sync-rule-save')
      await ctx.waitFor('the saved dialog closes', () =>
        gone(instance, 'files-sync-rule-form'),
      )
      const rules = unwrap<
        {
          rule: {
            name: string
            sourceKind: string
            source: { path: string }
            target: { path: string }
          }
        }[]
      >('sync_rules_list', await instance.invoke('sync_rules_list'))
      const saved = rules.find((view) => view.rule.name === entry)?.rule
      assert.ok(saved)
      assert.equal(saved.sourceKind, kind)
      assert.equal(saved.source.path, files.path(entry))
      assert.equal(saved.target.path, target.path)
      await instance.waitForDisplayed(`files-entry-${FILE}`)
      if (
        await instance.exec<boolean>(
          `return document.querySelector('[data-testid="files-sidebar-toggle"]').getAttribute('aria-expanded') !== 'true'`,
        )
      )
        await instance.click('files-sidebar-toggle')
      await instance.click('files-sync-rules-open')
      await instance.waitForDisplayed(`files-sync-rule-${entry}`)
      await instance.click('files-back')
      await instance.waitForDisplayed('files-entry-alpha')
    }
    ctx.step(
      'folder and file rules save with their prefilled sources and appear in the list',
    )

    // Phones have no split view (spec 062).
    if (!(await gone(instance, 'files-split-toggle'))) {
      if (
        await instance.exec<boolean>(
          `return document.querySelector('[data-testid="files-sidebar-toggle"]').getAttribute('aria-expanded') === 'true'`,
        )
      )
        await instance.click('files-sidebar-toggle')
      await instance.click('files-split-toggle')
      await instance.waitForDisplayed(entryIn('right', 'alpha'))
      await doubleClick(instance, entryIn('right', 'alpha'))
      await instance.waitForDisplayed(entryIn('right', 'in-alpha.txt'))
      await contextMenu(instance, inPanel('right', 'files-entries'))
      await instance.click('files-menu-syncRule')
      await instance.waitForDisplayed('files-picker-source-chosen')
      assert.equal(
        await textOf(instance, 'files-picker-source-path'),
        files.path('alpha'),
      )
      await closeModal(instance, ctx)
      await instance.waitForDisplayed(entryIn('left', FILE))
      ctx.step(
        'the right panel supplies its own folder without moving the left panel',
      )
      await instance.click('files-split-toggle')
    }

    const ownedPath = unwrap<string>(
      'plugin:path|resolve_directory',
      await instance.invoke('plugin:path|resolve_directory', { directory: 14 }),
    )
    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/device?p=${encodeURIComponent(ownedPath)}`,
    })
    await instance.waitForDisplayed(
      '[data-files-panel][data-active="true"] [data-files-list]',
    )
    await contextMenu(
      instance,
      '[data-files-panel][data-active="true"] [data-files-list]',
    )
    await instance.waitForDisplayed('files-context-menu')
    await ctx.waitFor('the protected folder facts are loaded', () =>
      instance.exec<boolean>(
        `return document.querySelector('[data-testid="files-menu-newFolder"]')?.getAttribute('aria-disabled') === 'true'`,
      ),
    )
    assert.equal(await gone(instance, 'files-menu-syncRule'), true)
    ctx.step('holzi’s own folder never offers a sync rule')
  } finally {
    files.remove()
  }
})
