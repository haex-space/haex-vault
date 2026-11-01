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
import { runAction, setSessionRestore, wmSnapshot } from '../lib/settings.ts'
import { openVault } from '../lib/sync-flows.ts'
import {
  attach,
  cleartextOf,
  detach,
  lockEncrypted,
  makeLockedStick,
  makeStick,
  mount,
  mountPointByLabel,
  mountPointOf,
  unmount,
  writeOnDrive,
} from '../lib/udisks.ts'

// Spec 064 US1 (T035), quickstart Linux 1: a stick plugged in while the file browser is open shows
// up in the sidebar without a reload, opens with one click (mounted first), and a tab on it moves to
// a folder that still exists once it is gone. The stick is a vfat loop image attached through
// udisks2, so the scenario needs one the runner user may drive.

const LABEL = 'E2EFOTOS'
/** SC-001: a plugged-in drive shows in the sidebar within this time. */
const APPEAR_LIMIT_MS = 3_000
const HARD_FACTOR = 2

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

    // Plugged in with the window open: the stick appears on its own within 3 s (SC-001). As in
    // `files-large-folder`, the shared runner may take up to twice the limit.
    ctx.step('plug in', stick.file)
    const plugged = performance.now()
    device = attach(stick.file)
    const row = await ctx.waitFor(
      'the stick in the sidebar',
      () => driveRow(instance, LABEL),
      { timeoutMs: APPEAR_LIMIT_MS * HARD_FACTOR, intervalMs: 50, fixed: true },
    )
    assert.ok(row)
    const appearedMs = Math.round(performance.now() - plugged)
    assert.ok(
      appearedMs <= APPEAR_LIMIT_MS * HARD_FACTOR,
      `the stick took ${appearedMs} ms to appear`,
    )
    ctx.step('the stick in the sidebar', `${appearedMs} ms`)

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

    /**
     * Waits until the stick is unlocked and the dialog gone. The folder view is no signal: after an
     * eject it still lists the stick's files until the window moves on.
     */
    const waitOpened = async (what: string) => {
      await ctx.waitFor(`${what}: unlocked`, () =>
        cleartextOf(stick.device) ? true : null,
      )
      await ctx.waitFor(`${what}: the dialog closed`, () =>
        gone(instance, 'files-drive-unlock-form'),
      )
      await instance.waitForDisplayed('files-entry-sicherung.txt', 30_000)
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
    await waitOpened('typed')
    await ejectAndWait('typed')
    ctx.step('unlocked with the typed password, locked again on eject')

    // US4: remembered, it opens the next time without a dialog.
    await instance.click(locked)
    await instance.type('#files-drive-unlock-password', BACKUP_PASSWORD)
    await instance.click('files-drive-unlock-remember')
    await instance.click('files-drive-unlock-submit')
    await waitOpened('remembered')
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

    // Unlocked without anybody typing: the dialog would have waited for a password.
    await instance.click(locked)
    await waitOpened('saved')
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

// Spec 064 FR-025 (T064), quickstart Linux 7: a tab stands in a folder of a stick and of an
// encrypted stick; haex ends, the system unmounts the one and locks the other. Restored, each tab
// says so instead of moving up, mounts nothing unasked, and its button opens the same folder: the
// stick mounted, the encrypted one with the password remembered before.

const PLAIN = 'E2ERESTORE'
const SAFE = 'E2ESAFE'
const SAFE_PASSWORD = 'e2e-restore-password'

scenario(
  'files-drives-restore',
  { needs: { udisks: true, luks: true }, timeoutMs: 300_000 },
  async (ctx) => {
    const plain = makeStick(PLAIN)
    let plainDevice: string | null = null
    ctx.onTeardown(() => {
      if (plainDevice) detach(plainDevice)
      plain.remove()
    })
    const safe = makeLockedStick(SAFE, SAFE_PASSWORD, {
      'ordner/geheim.txt': 'verschlossen',
    })
    ctx.onTeardown(() => safe.remove())
    plainDevice = attach(plain.file)
    const vault = { name: 'e2e-files-drives-restore', passphrase: 'e2e-vault' }

    const instance = await ctx.startInstance()
    await createAndUnlock(instance, vault)
    await setSessionRestore(instance, true)

    // The encrypted stick: unlocked with "merken", then into its folder.
    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: '/device',
    })
    const locked = await ctx.waitFor('the locked stick in the sidebar', () =>
      driveRow(instance, SAFE),
    )
    assert.ok(locked)
    await instance.click(locked)
    await instance.type('#files-drive-unlock-password', SAFE_PASSWORD)
    await instance.click('files-drive-unlock-remember')
    await instance.click('files-drive-unlock-submit')
    await ctx.waitFor('the encrypted stick unlocked', () =>
      cleartextOf(safe.device) ? true : null,
    )
    await doubleClick(instance, 'files-entry-ordner', 30_000)
    await instance.waitForDisplayed('files-entry-geheim.txt', 15_000)

    // The plain stick, mounted by the system and opened in a tab of its own from outside: the tab
    // learns its drive from the list.
    const plainRoot = mount(plainDevice)
    writeOnDrive(plainRoot, 'ordner/inhalt.txt', 'eingehängt')
    const opened = await runAction(instance, 'wm.tab.new', {
      appId: 'system.files',
      at: `/device?p=${encodeURIComponent(`${plainRoot}/ordner`)}`,
    })
    assert.ok(opened.ok, JSON.stringify(opened))
    await instance.waitForDisplayed('files-entry-inhalt.txt', 15_000)
    ctx.step('two tabs on drives')

    const safeRoot = mountPointByLabel(SAFE)
    assert.ok(safeRoot, 'the encrypted stick mounted')
    await ctx.waitFor(
      'the saved session to know both drives',
      async () => {
        const text = JSON.stringify(await instance.invoke('wm_session_load'))
        return [plainRoot, safeRoot].every((root) =>
          text.includes(`"droot":${JSON.stringify(root)}`),
        )
          ? true
          : null
      },
      { fixed: true, timeoutMs: 15_000 },
    )

    await instance.stop()
    unmount(plainDevice)
    lockEncrypted(safe.device)
    assert.equal(mountPointOf(plainDevice), null)
    assert.equal(cleartextOf(safe.device), null)
    ctx.step('haex ended, the stick unmounted, the encrypted one locked')

    const restored = await ctx.startInstance({ reusesRoot: instance.root })
    await openVault(restored, vault.name, vault.passphrase)
    const tabs = (await ctx.waitFor(
      'two file browser tabs after the restart',
      async () => {
        const ids = (await wmSnapshot(restored)).windows.flatMap((window) =>
          window.tabs
            .filter((tab) => tab.appId === 'system.files')
            .map((tab) => tab.id),
        )
        return ids.length === 2 ? ids : false
      },
      { fixed: true, timeoutMs: 30_000 },
    )) as string[]

    for (const tabId of tabs) {
      const activated = await runAction(restored, 'wm.tab.activate', { tabId })
      assert.ok(activated.ok, JSON.stringify(activated))
      const text = (await ctx.waitFor('the tab to show its drive notice', () =>
        restored.exec<string | false>(
          `const el = [...document.querySelectorAll('[data-testid="files-drive-notice"]')]
             .find((candidate) => candidate.getClientRects().length > 0)
           return el ? el.textContent : false`,
        ),
      )) as string
      // Nothing was mounted or unlocked unasked: this tab's drive is still closed (the other tab's
      // may be open by now, through its own click).
      if (text.includes(PLAIN)) {
        assert.match(text, /nicht eingehängt/)
        assert.equal(mountPointOf(plainDevice), null)
        await restored.click('files-drive-notice-open')
        await restored.waitForDisplayed('files-entry-inhalt.txt', 30_000)
        ctx.step('the stick mounted, the folder open')
      } else {
        assert.match(text, new RegExp(`${SAFE}.*gesperrt`))
        assert.equal(cleartextOf(safe.device), null)
        await restored.click('files-drive-notice-open')
        await restored.waitForDisplayed('files-entry-geheim.txt', 30_000)
        assert.ok(
          await gone(restored, 'files-drive-unlock-form'),
          'the saved password opened it',
        )
        ctx.step('the encrypted stick unlocked with its saved password')
      }
    }
  },
)
