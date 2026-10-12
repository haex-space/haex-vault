import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap, type FlowInstance } from '../lib/flows.ts'
import type { WaitContext } from '../lib/sync-flows.ts'
import { dialogClosed } from '../lib/appearance.ts'
import { KEY, resizeAppWindow, runAction, wmSnapshot } from '../lib/settings.ts'
import {
  contextMenu,
  createEntry,
  createFolder,
  dragTo,
  historyIds,
  openPasswords,
  overview,
  placeOf,
  rowDimmed,
  selectedRows,
  updateEntry,
} from '../lib/passwords.ts'

// Spec 036, quickstart M3 (US3 without copy, US4): the breadcrumbs lead back up the path; a
// selection is cut into the Ablage, dimmed, and pasted into another folder, which empties the
// Ablage; a folder cannot be pasted into its own subfolder (nothing moves, the Ablage stays); a
// second window of the password manager shares the Ablage; a drop on a part of the breadcrumbs moves
// the whole selection. The context menus of an entry, a folder, the empty area and the trash offer
// their actions; the list works by keyboard (select all, extend, cut, open, paste, delete, copy the
// password); on a narrow window the menu button of a row replaces the right click.

/** Clicks a part of the breadcrumbs, also when it sits in the overflow menu of a narrow row. Waits
 * for the breadcrumbs first: they fade back in after the selection bar. */
async function clickCrumb(
  ctx: WaitContext,
  instance: FlowInstance,
  id: string,
): Promise<void> {
  const hook = `passwords-crumb-${id}`
  const where = await ctx.waitFor(`the breadcrumb ${id}`, () =>
    instance.exec<'shown' | 'more' | false>(
      `const shown = (testid) => {
         const el = document.querySelector('[data-testid="' + testid + '"]')
         return Boolean(el && el.getClientRects().length)
       }
       return shown(arguments[0]) ? 'shown' : shown('passwords-crumb-more') ? 'more' : false`,
      [hook],
    ),
  )
  if (where === 'more') await instance.click('passwords-crumb-more')
  await instance.click(hook)
}

async function title(instance: FlowInstance): Promise<string> {
  return instance.exec<string>(
    `return document.querySelector('[data-testid="passwords-title"]')?.textContent?.trim() ?? ''`,
  )
}

async function count(instance: FlowInstance, selector: string) {
  return instance.exec<number>(
    `return document.querySelectorAll(arguments[0]).length`,
    [selector],
  )
}

scenario('passwords-organize', { timeoutMs: 300_000 }, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-passwords-organize' })
  await openPasswords(instance)
  const work = await createFolder(instance, 'Arbeit')
  const server = await createFolder(instance, 'Server', work)
  const db = await createFolder(instance, 'DB', server)
  const home = await createFolder(instance, 'Privat')
  const mail = await createEntry(
    instance,
    { title: 'Mail', username: 'anna', password: 'SECRET-MARKER-E2E-ORG' },
    db,
  )
  const git = await createEntry(instance, { title: 'Git', username: 'a' }, db)
  const bank = await createEntry(instance, { title: 'Bank', username: 'b' })

  // Down through the folder rows of the list; the breadcrumbs name the path.
  await instance.click(`passwords-list-folder-${work}`)
  await instance.click(`passwords-list-folder-${server}`)
  await instance.click(`passwords-list-folder-${db}`)
  await ctx.waitFor(
    'the title DB',
    async () => (await title(instance)) === 'DB',
  )
  await instance.waitForDisplayed('passwords-crumb-root')
  await clickCrumb(ctx, instance, work)
  await ctx.waitFor(
    'the title Arbeit',
    async () => (await title(instance)) === 'Arbeit',
  )
  assert.ok((await runAction(instance, 'wm.tab.back')).ok)
  await ctx.waitFor('DB again', async () => (await title(instance)) === 'DB')
  ctx.step('breadcrumbs name the path and lead back up')

  // Select both entries with Ctrl+A, cut them: the Ablage shows, the rows are dimmed.
  await instance.type(
    `passwords-entry-${mail}`,
    KEY.control + 'a' + KEY.release,
  )
  await ctx.waitFor(
    'both entries selected',
    async () =>
      (await selectedRows(instance)).sort().join() ===
      [git, mail].sort().join(),
  )
  await instance.click('passwords-selection-cut')
  await instance.waitForDisplayed('passwords-ablage')
  assert.equal(await rowDimmed(instance, mail), true, 'a cut row is dimmed')
  assert.deepEqual(
    await selectedRows(instance),
    [],
    'cutting ends the selection',
  )
  ctx.step('cut: the Ablage shows and the rows are dimmed')

  // Paste into Privat: they moved, and the Ablage is empty.
  await clickCrumb(ctx, instance, 'root')
  await instance.click(`passwords-list-folder-${home}`)
  await ctx.waitFor(
    'the title Privat',
    async () => (await title(instance)) === 'Privat',
  )
  await instance.click('passwords-ablage-paste')
  await ctx.waitFor(
    'both entries in Privat',
    async () =>
      (await placeOf(instance, mail)) === home &&
      (await placeOf(instance, git)) === home,
  )
  await ctx.waitFor(
    'the Ablage to empty',
    async () =>
      (await count(instance, '[data-testid="passwords-ablage"]')) === 0,
  )
  ctx.step('paste moves them and empties the Ablage')

  // A folder into its own subfolder: refused, nothing moves, the Ablage stays.
  await clickCrumb(ctx, instance, 'root')
  await contextMenu(instance, `passwords-list-folder-${work}`)
  await instance.click('passwords-menu-cut')
  await instance.waitForDisplayed('passwords-ablage')
  await instance.click(`passwords-list-folder-${work}`)
  await instance.click(`passwords-list-folder-${server}`)
  await ctx.waitFor(
    'the title Server',
    async () => (await title(instance)) === 'Server',
  )
  await instance.click('passwords-ablage-paste')
  await ctx.waitFor(
    'the refusal',
    async () =>
      (await count(instance, '[data-sonner-toast][data-type="error"]')) > 0,
  )
  assert.equal(await placeOf(instance, work), null, 'the folder did not move')
  await instance.waitForDisplayed('passwords-ablage')
  ctx.step('a folder into its own subfolder is refused and the Ablage stays')

  // A second window of the password manager shares the Ablage.
  const opened = await runAction(instance, 'wm.app.open', {
    appId: 'system.passwords',
  })
  assert.ok(opened.ok, JSON.stringify(opened))
  await ctx.waitFor(
    'the Ablage in both windows',
    async () =>
      (await count(instance, '[data-testid="passwords-ablage"]')) === 2,
  )
  ctx.step('a second window shares the Ablage')
  const second = (await wmSnapshot(instance)).windows
    .filter((window) =>
      window.tabs.some((tab) => tab.appId === 'system.passwords'),
    )
    .at(-1)
  assert.ok(second, 'the second window is listed')
  const closed = await runAction(instance, 'wm.window.close', {
    windowId: second.id,
  })
  assert.ok(closed.ok, JSON.stringify(closed))
  await ctx.waitFor(
    'one window again',
    async () => (await count(instance, '[data-passwords-app]')) === 1,
  )
  await instance.click('passwords-ablage-clear')

  // A drop of a selected entry on a part of the breadcrumbs moves the whole selection.
  await clickCrumb(ctx, instance, 'root')
  await instance.click(`passwords-list-folder-${home}`)
  await ctx.waitFor(
    'the title Privat',
    async () => (await title(instance)) === 'Privat',
  )
  await instance.type(
    `passwords-entry-${mail}`,
    KEY.control + 'a' + KEY.release,
  )
  await ctx.waitFor(
    'both selected',
    async () => (await selectedRows(instance)).length === 2,
  )
  await dragTo(instance, `passwords-entry-${mail}`, 'passwords-crumb-root')
  await ctx.waitFor(
    'both at the top level',
    async () =>
      (await placeOf(instance, mail)) === null &&
      (await placeOf(instance, git)) === null,
  )
  ctx.step('a drop on the breadcrumbs moves the whole selection')

  // Part 2: the menus. An entry: open, copy username/password, cut, delete. The selection ends
  // when its rows leave Privat; the breadcrumbs come back after the selection bar.
  await ctx.waitFor(
    'the selection to end',
    async () => (await selectedRows(instance)).length === 0,
  )
  await ctx.waitFor('the top level', async () => {
    const shown = await instance.exec<boolean>(
      `const el = document.querySelector('[data-testid="passwords-entry-' + arguments[0] + '"]')
       return Boolean(el && el.getClientRects().length)`,
      [bank],
    )
    if (!shown) await clickCrumb(ctx, instance, 'root').catch(() => undefined)
    return shown
  })
  await contextMenu(instance, `passwords-entry-${bank}`)
  for (const id of ['open', 'copyUsername', 'copyPassword', 'cut', 'delete']) {
    await instance.waitForDisplayed(`passwords-menu-${id}`)
  }
  assert.equal(
    await instance.exec<boolean>(
      `return document.querySelector('[data-testid="passwords-menu-copyPassword"]').hasAttribute('data-disabled')`,
    ),
    true,
    'copy password is greyed out without a password',
  )
  await instance.typeToFocused(KEY.escape)
  // A folder: open, edit, new subfolder, cut, delete; new subfolder opens the folder form.
  await contextMenu(instance, `passwords-list-folder-${home}`)
  for (const id of ['open', 'edit', 'newSubfolder', 'cut', 'delete']) {
    await instance.waitForDisplayed(`passwords-menu-${id}`)
  }
  await instance.click('passwords-menu-newSubfolder')
  await instance.waitForDisplayed('passwords-folder-form')
  await instance.typeToFocused(KEY.escape)
  // The empty area: new entry and new folder.
  await contextMenu(instance, 'passwords-list-area')
  await instance.waitForDisplayed('passwords-menu-newEntry')
  await instance.waitForDisplayed('passwords-menu-newFolder')
  await instance.typeToFocused(KEY.escape)
  // The trash of the sidebar: empty the trash.
  await contextMenu(instance, 'passwords-trash')
  await instance.waitForDisplayed('passwords-menu-emptyTrash')
  await instance.typeToFocused(KEY.escape)
  ctx.step('menus of an entry, a folder, the empty area and the trash')

  // By keyboard: from Bank extend over the entries, cut, open Privat, paste.
  await instance.type(
    `passwords-entry-${bank}`,
    KEY.shift + KEY.arrowDown + KEY.arrowDown + KEY.release,
  )
  await ctx.waitFor(
    'three entries selected by Shift+Arrow',
    async () => (await selectedRows(instance)).length === 3,
  )
  await instance.typeToFocused(KEY.control + 'x' + KEY.release)
  await instance.waitForDisplayed('passwords-ablage')
  await instance.type(`passwords-list-folder-${home}`, KEY.enter)
  await ctx.waitFor(
    'the title Privat',
    async () => (await title(instance)) === 'Privat',
  )
  await instance.typeToFocused(KEY.control + 'v' + KEY.release)
  await ctx.waitFor(
    'the three entries in Privat',
    async () =>
      (await placeOf(instance, bank)) === home &&
      (await placeOf(instance, mail)) === home &&
      (await placeOf(instance, git)) === home,
  )
  ctx.step('select, cut, open and paste by keyboard')

  // Delete asks first; Ctrl+Shift+C copies the password with the usual answer.
  await instance.type(`passwords-entry-${mail}`, KEY.delete)
  await instance.waitForDisplayed('passwords-delete-confirm')
  await instance.typeToFocused(KEY.escape)
  // The dialog gives the focus back to Mail's row when its closing animation ends; a Delete typed on
  // another row before that could land on Mail (it once deleted Mail instead of Git below).
  await ctx.waitFor('the delete dialog closed', () => dialogClosed(instance))
  // Earlier toasts are marked, so a new one counts even when an older one expires meanwhile
  // (comparing the number of toasts failed then).
  await instance.exec(
    `document.querySelectorAll('[data-sonner-toast]').forEach((toast) => toast.setAttribute('data-e2e-seen', ''))`,
    [],
  )
  await instance.type(
    `passwords-entry-${mail}`,
    KEY.control + KEY.shift + 'c' + KEY.release,
  )
  await ctx.waitFor(
    'the copy answer',
    async () =>
      (await count(
        instance,
        '[data-sonner-toast][data-type="success"]:not([data-e2e-seen])',
      )) > 0,
  )
  ctx.step('Delete asks, Ctrl+Shift+C copies the password')

  // A confirmed delete takes the focused row away; the focus comes back to the list, so the
  // shortcuts still work without a click.
  await instance.type(`passwords-entry-${git}`, KEY.delete)
  await instance.click('passwords-delete-confirm')
  await ctx.waitFor(
    'the deleted row gone and the dialog closed',
    async () =>
      (await count(instance, `[data-testid="passwords-entry-${git}"]`)) === 0
        ? dialogClosed(instance)
        : false,
    { timeoutMs: 30_000, fixed: true },
  )
  await ctx.waitFor('the focus back in the list', () =>
    instance.exec<boolean>(
      `return Boolean(document.activeElement?.closest('[data-passwords-list]'))`,
    ),
  )
  await instance.typeToFocused(KEY.control + 'a' + KEY.release)
  const rows = await count(instance, '[data-passwords-list] [data-row-id]')
  await ctx.waitFor(
    'Ctrl+A after the delete',
    async () => rows > 0 && (await selectedRows(instance)).length === rows,
  )
  await instance.typeToFocused(KEY.escape)
  ctx.step('after a delete the focus returns to the list')

  // A narrow window: the menu button of the row opens the same menu.
  await resizeAppWindow(instance, 'system.passwords', 360, 560)
  await instance.click(`passwords-entry-menu-${mail}`)
  await instance.waitForDisplayed('passwords-menu-open')
  await instance.typeToFocused(KEY.escape)
  ctx.step('at 360 px the row menu button replaces the right click')

  // Part 3 (stage 3): copying with the dialog. A folder copy: cancel makes nothing, confirm makes
  // the copy with the suffix, and the Ablage stays for another paste.
  await resizeAppWindow(instance, 'system.passwords', 760, 560)
  await clickCrumb(ctx, instance, 'root')
  await contextMenu(instance, `passwords-list-folder-${home}`)
  await instance.click('passwords-menu-copy')
  await instance.waitForDisplayed('passwords-ablage')
  await instance.click(`passwords-list-folder-${work}`)
  await instance.click('passwords-ablage-paste')
  await instance.waitForDisplayed('passwords-copy-suffix')
  await instance.click('passwords-copy-cancel')
  const groupsNamed = async (prefix: string) =>
    (await overview(instance)).groups.filter((group) =>
      (group.name ?? '').startsWith(prefix),
    )
  assert.equal((await groupsNamed('Privat ')).length, 0, 'cancel makes nothing')
  // Privat holds what is left of it after the delete above.
  const inPrivat = (await overview(instance)).headers.filter(
    (h) => h.groupId === home,
  ).length
  await instance.click('passwords-ablage-paste')
  await instance.click('passwords-copy-confirm')
  await ctx.waitFor('the copy of the folder in Arbeit', async () => {
    const [copy] = await groupsNamed('Privat ')
    if (!copy || copy.parentId !== work) return false
    return (
      (await overview(instance)).headers.filter((h) => h.groupId === copy.id)
        .length === inPrivat
    )
  })
  await instance.waitForDisplayed('passwords-ablage')
  ctx.step('a folder copy through the dialog; cancel makes nothing')

  // An entry copy with the password as a reference and the history taken.
  await updateEntry(instance, mail, { password: 'SECRET-MARKER-E2E-ORG-2' })
  await clickCrumb(ctx, instance, 'root')
  await instance.click(`passwords-list-folder-${home}`)
  await contextMenu(instance, `passwords-entry-${mail}`)
  await instance.click('passwords-menu-copy')
  await instance.click('passwords-ablage-paste')
  await instance.waitForDisplayed('passwords-copy-title')
  await instance.click('passwords-copy-password-reference')
  await instance.click('passwords-copy-history')
  await instance.click('passwords-copy-confirm')
  const copyId = (await ctx.waitFor('the copy of Mail', async () => {
    const copy = (await overview(instance)).headers.find(
      (h) => h.title !== 'Mail' && (h.title ?? '').startsWith('Mail'),
    )
    return copy?.id ?? false
  })) as string
  const revealed = async (id: string) =>
    unwrap<{ value: string }>(
      'passwords_reveal',
      await instance.invoke('passwords_reveal', {
        args: { itemId: id, field: { kind: 'password' } },
      }),
    ).value
  assert.equal(await revealed(copyId), 'SECRET-MARKER-E2E-ORG-2')
  await updateEntry(instance, mail, { password: 'SECRET-MARKER-E2E-ORG-3' })
  assert.equal(await revealed(copyId), 'SECRET-MARKER-E2E-ORG-3')
  assert.ok(
    (await historyIds(instance, copyId)).length >= 3,
    'the copy took the history',
  )
  ctx.step('an entry copy follows the original and took the history')
})
