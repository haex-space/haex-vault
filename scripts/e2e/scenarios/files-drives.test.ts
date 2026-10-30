import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { createAndUnlock, unwrap, type FlowInstance } from '../lib/flows.ts'
import {
  contextMenu,
  doubleClick,
  overview,
  updateEntry,
} from '../lib/passwords.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'
import {
  attach,
  cleartextOf,
  detach,
  makeLockedStick,
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

    // A file of it open in the viewer does not hold it: haex lets go first (FR-009). A loop image
    // has no drive to power off, so the release left it attached and only unmounted: the same row
    // mounts it again. A second loop-setup would show the stick twice and leave one behind. A click
    // before the sidebar has heard of the unmount would only open the old path.
    await ctx.waitFor('the sidebar showing it unmounted', () =>
      instance.exec<boolean>(
        `const row = document.querySelector('[data-testid="' + arguments[0] + '"]')
         return !!row && row.getAttribute('aria-busy') !== 'true'
           && !document.querySelector('[data-testid="' + arguments[1] + '"]')`,
        [row, eject],
      ),
    )
    await instance.click(row)
    await ctx.waitFor('the remounted stick', () =>
      device ? mountPointOf(device) : null,
    )
    await doubleClick(instance, 'files-entry-lies.txt')
    await instance.waitForDisplayed('files-viewer-text')
    await instance.click(eject, 15_000)
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

// Spec 064 US3 and US4 (T052, T058), quickstart Linux 4 and 5: an encrypted stick shows up locked,
// opens through the dialog with its password (a wrong one keeps the dialog open), is locked again on
// eject, and with "remember" opens the next time without a dialog, until its saved password is wrong.
const BACKUP = 'E2EBACKUP'
const BACKUP_PASSWORD = 'e2e-drive-password'

scenario(
  'files-drives-unlock',
  { needs: { udisks: true, luks: true }, timeoutMs: 240_000 },
  async (ctx) => {
    const stick = makeLockedStick(BACKUP, BACKUP_PASSWORD, {
      'sicherung.txt': 'gesichert',
    })
    ctx.onTeardown(() => stick.remove())

    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-drives-unlock' })
    await runAction(instance, 'wm.app.open', { appId: 'system.files' })
    const locked = await ctx.waitFor('the locked stick in the sidebar', () =>
      driveRow(instance, BACKUP),
    )
    assert.ok(locked)

    /** Ejects the open stick and waits until the sidebar shows it locked again. */
    const ejectAndWait = async (what: string) => {
      const open = await ctx.waitFor(`${what}: the open stick`, async () => {
        const row = await driveRow(instance, BACKUP)
        return row !== locked ? row : null
      })
      assert.ok(open)
      await instance.click(
        open.replace('files-drive-', 'files-drive-eject-'),
        15_000,
      )
      await ctx.waitFor(`${what}: locked again`, () =>
        cleartextOf(stick.device) === null ? true : null,
      )
      await ctx.waitFor(`${what}: the sidebar showing it locked`, async () =>
        (await driveRow(instance, BACKUP)) === locked ? true : null,
      )
    }

    // US3: a wrong password keeps the dialog open with the system's text; the field is emptied.
    await instance.click(locked)
    await instance.type('#files-drive-unlock-password', 'falsch')
    await instance.click('files-drive-unlock-submit')
    await instance.waitForDisplayed('files-drive-unlock-error', 30_000)
    assert.equal(cleartextOf(stick.device), null)
    assert.equal(
      await instance.exec<string>(
        `return document.getElementById('files-drive-unlock-password')?.value ?? null`,
      ),
      '',
    )
    ctx.step('a wrong password keeps the dialog')

    await instance.type('#files-drive-unlock-password', BACKUP_PASSWORD)
    await instance.click('files-drive-unlock-submit')
    await instance.waitForDisplayed('files-entry-sicherung.txt', 30_000)
    assert.ok(cleartextOf(stick.device))
    await ejectAndWait('typed')
    ctx.step('unlocked with the typed password, locked again on eject')

    // US4: remembered, it opens the next time without a dialog.
    await instance.click(locked)
    await instance.type('#files-drive-unlock-password', BACKUP_PASSWORD)
    await instance.click('files-drive-unlock-remember')
    await instance.click('files-drive-unlock-submit')
    await instance.waitForDisplayed('files-entry-sicherung.txt', 30_000)
    const saved = (await overview(instance)).headers.find(
      (header) => header.title === `Laufwerk: ${BACKUP}`,
    )
    assert.ok(saved, 'the entry in the password manager')
    assert.ok(saved.url?.startsWith('haex:drive:'), saved.url ?? 'no address')
    assert.deepEqual(
      saved.tags.map((tag) => tag.name),
      ['haex-drive'],
    )
    await ejectAndWait('remembered')
    ctx.step('remembered in the password manager')

    await instance.click(locked)
    await instance.waitForDisplayed('files-entry-sicherung.txt', 30_000)
    assert.ok(
      await gone(instance, 'files-drive-unlock-form'),
      'no dialog with a saved password',
    )
    await ejectAndWait('saved')
    ctx.step('opened with the saved password')

    // A saved password that no longer fits opens the dialog with the hint.
    await updateEntry(instance, saved.id, { password: 'nicht mehr' })
    await instance.click(locked)
    await instance.waitForDisplayed('files-drive-unlock-hint', 30_000)
    assert.equal(cleartextOf(stick.device), null)
    ctx.step('a saved password that fails asks again')
  },
)
