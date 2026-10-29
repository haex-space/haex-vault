import assert from 'node:assert/strict'

import { createAndUnlock, type FlowInstance } from '../lib/flows.ts'
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
