import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { createAndUnlock, unwrap, type FlowInstance } from '../lib/flows.ts'
import { contextMenu, doubleClick } from '../lib/passwords.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'
import {
  attach,
  detach,
  makeStick,
  mountPointOf,
  writeOnDrive,
} from '../lib/udisks.ts'

// Spec 064 US1 (T035), quickstart Linux 1: a stick plugged in while the file browser is open shows
// up in the sidebar without a reload, opens with one click (mounted first), and a tab on it moves to
// a folder that still exists once it is gone. The stick is a vfat loop image attached through
// udisks2, so the scenario needs one the runner user may drive.

const LABEL = 'E2EFOTOS'

/** The test id of the sidebar row labelled `label` (`files-drive-<id>-<label>`), or `null`. */
const driveRow = (instance: FlowInstance, label: string) =>
  instance.exec<string | null>(
    `const row = [...document.querySelectorAll('[data-testid^="files-drive-"]')]
       .find((el) => el.getAttribute('data-testid').endsWith('-' + arguments[0]))
     return row ? row.getAttribute('data-testid') : null`,
    [label],
  )

const gone = (instance: FlowInstance, hook: string) =>
  instance.exec<boolean>(
    `return !document.querySelector('[data-testid="' + arguments[0] + '"]')`,
    [hook],
  )

/** Clicks in the page itself: an open dialog of a waiting transfer keeps pointer clicks out. */
const pageClick = (instance: FlowInstance, hook: string) =>
  instance.exec<boolean>(
    `const el = document.querySelector('[data-testid="' + arguments[0] + '"]')
     if (el) el.click()
     return Boolean(el)`,
    [hook],
  )

const toastText = (instance: FlowInstance) =>
  instance.exec<string>(
    `return [...document.querySelectorAll('[data-sonner-toast]')]
       .map((el) => el.textContent).join('\\n')`,
  )

scenario(
  'files-drives',
  { needs: { udisks: true }, timeoutMs: 180_000 },
  async (ctx) => {
    const stick = makeStick(LABEL)
    let device: string | null = null
    ctx.onTeardown(() => {
      if (device) detach(device)
      stick.remove()
    })

    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-drives' })
    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: '/device',
    })
    await instance.waitForDisplayed('files-sidebar')

    // Plugged in with the window open: the stick appears on its own (SC-001: 3 s, scaled for the
    // runner like every wait).
    ctx.step('plug in', stick.file)
    device = attach(stick.file)
    const row = await ctx.waitFor(
      'the stick in the sidebar',
      () => driveRow(instance, LABEL),
      { timeoutMs: 3_000, intervalMs: 100 },
    )
    assert.ok(row)

    // One click mounts it and opens it (FR-006).
    ctx.step('open', row)
    await instance.click(row)
    const mounted = await ctx.waitFor('the stick mounted', () =>
      device ? mountPointOf(device) : null,
    )
    assert.ok(mounted)
    writeOnDrive(mounted, 'hallo.txt', 'vom Stick')
    await instance.click('files-reload')
    await instance.waitForDisplayed('files-entry-hallo.txt', 15_000)

    // Pulled while a tab shows it: it leaves the sidebar and the tab moves up to a folder that
    // exists (spec 044 edge case, spec 064 US1 scenario 7).
    ctx.step('pull', device)
    detach(device)
    device = null
    await ctx.waitFor(
      'the stick gone from the sidebar',
      async () => (await driveRow(instance, LABEL)) === null,
    )
    await ctx.waitFor('the notice about the vanished folder', async () =>
      /gibt es nicht mehr|no longer exists/.test(await toastText(instance)),
    )
    assert.equal(
      await instance.exec<boolean>(
        `return Boolean(document.querySelector('[data-testid="files-entry-hallo.txt"]'))`,
      ),
      false,
    )
  },
)

// Spec 064 US2 (T045), quickstart Linux 3: the root has no button; with a copy onto the stick
// waiting, the button asks first and a no keeps it mounted; with a file of it open in the viewer,
// the button unmounts it and says so.
scenario(
  'files-drives-eject',
  { needs: { udisks: true }, timeoutMs: 180_000 },
  async (ctx) => {
    const stick = makeStick('E2EAUSWURF')
    const files = deviceFiles('haex-eject-')
    let device: string | null = null
    ctx.onTeardown(() => {
      if (device) detach(device)
      stick.remove()
      files.remove()
    })
    files.write('notiz.txt', 'neu')

    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-drives-eject' })
    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/device?p=${encodeURIComponent(files.folder)}`,
    })
    await instance.waitForDisplayed('files-entry-notiz.txt')

    device = attach(stick.file)
    const row = await ctx.waitFor('the stick in the sidebar', () =>
      driveRow(instance, 'E2EAUSWURF'),
    )
    assert.ok(row)

    // The root is the system's: no button (FR-008).
    const sources = unwrap<{
      drives: { id: string; label: string | null; path: string | null }[]
    }>('files_sources', await instance.invoke('files_sources', {}))
    const root = sources.drives.find((drive) => drive.path === '/')
    if (root) {
      assert.ok(
        await instance.exec<boolean>(
          `const id = 'files-drive-eject-' + arguments[0] + '-' + (arguments[1] ?? arguments[0])
           return ![...document.querySelectorAll('[data-testid^="files-drive-eject-"]')]
             .some((el) => el.getAttribute('data-testid') === id)`,
          [root.id, root.label],
        ),
        'no button for the root',
      )
    }
    ctx.step('the root has no button')

    // A copy onto the stick waits at a taken name: the button asks, and a no changes nothing.
    await contextMenu(instance, 'files-entry-notiz.txt')
    await instance.click('files-menu-copy')
    await instance.click(row)
    const mounted = await ctx.waitFor('the stick mounted', () =>
      device ? mountPointOf(device) : null,
    )
    assert.ok(mounted)
    writeOnDrive(mounted, 'notiz.txt', 'alt')
    writeOnDrive(mounted, 'lies.txt', 'vom Stick')
    await instance.waitForDisplayed('files-entry-lies.txt', 15_000)
    await instance.click('files-action-paste')
    await instance.waitForDisplayed('files-conflict')
    // The button comes once the sidebar has heard of the mount, which can trail the folder view.
    const eject = row.replace('files-drive-', 'files-drive-eject-')
    await instance.waitForDisplayed(eject, 15_000)
    assert.ok(await pageClick(instance, eject))
    await instance.waitForDisplayed('files-drive-release-confirm')
    assert.ok(await pageClick(instance, 'files-drive-release-cancel'))
    await ctx.waitFor('the question closed', () =>
      gone(instance, 'files-drive-release-confirm'),
    )
    await instance.click('files-conflict-cancel')
    await ctx.waitFor('the copy to end', () =>
      gone(instance, 'files-transfers'),
    )
    assert.equal(mountPointOf(device), mounted, 'a no keeps it mounted')
    ctx.step('running transfers: asked, no keeps it')

    // A yes cancels the pending transfer before releasing the drive (FR-010). The copy comes from
    // the home folder again: copied on the stick itself it would land as "notiz (2).txt".
    await instance.click('files-back')
    await ctx.waitFor('the home folder again', () =>
      gone(instance, 'files-entry-lies.txt'),
    )
    await contextMenu(instance, 'files-entry-notiz.txt')
    await instance.click('files-menu-copy')
    await instance.click(row)
    await instance.click('files-action-paste')
    await instance.waitForDisplayed('files-conflict')
    assert.ok(await pageClick(instance, eject))
    await instance.waitForDisplayed('files-drive-release-confirm')
    assert.ok(await pageClick(instance, 'files-drive-release-confirm'))
    await ctx.waitFor('the confirmed release cancels the transfer', () =>
      gone(instance, 'files-transfers'),
    )
    await ctx.waitFor('the confirmed release unmounts the drive', () =>
      device ? mountPointOf(device) === null : false,
    )
    ctx.step('running transfers: yes cancels them and releases the drive')

    // A file of it open in the viewer does not hold it: haex lets go first (FR-009).
    device = attach(stick.file)
    const remountedRow = await ctx.waitFor('the stick mounted again', () =>
      driveRow(instance, 'E2EAUSWURF'),
    )
    assert.ok(remountedRow)
    await instance.click(remountedRow)
    await ctx.waitFor('the remounted stick', () =>
      device ? mountPointOf(device) : null,
    )
    await doubleClick(instance, 'files-entry-lies.txt')
    await instance.waitForDisplayed('files-viewer-text')
    const remountedEject = remountedRow.replace(
      'files-drive-',
      'files-drive-eject-',
    )
    await instance.click(remountedEject, 15_000)
    await ctx.waitFor('the stick unmounted', () =>
      device ? mountPointOf(device) === null : false,
    )
    await ctx.waitFor('the notice', async () =>
      /kann abgezogen werden|ausgehängt|can be removed|unmounted/.test(
        await toastText(instance),
      ),
    )
    ctx.step('ejected with a file open')
  },
)
