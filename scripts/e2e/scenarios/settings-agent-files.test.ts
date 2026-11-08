import assert from 'node:assert/strict'
import { createAndUnlock, type FlowInstance } from '../lib/flows.ts'
import {
  editStandardRights,
  openStandardProfile,
  standardRights,
} from '../lib/agent-profiles.ts'
import { startRustfs } from '../lib/rustfs.ts'
import { scenario } from '../lib/scenario.ts'
import { choose } from '../lib/settings.ts'
import { connectStorage } from '../lib/storage-setup.ts'

// Spec 044 T077 (FR-031b), spec 083 FR-033: the group "Dateien" in the page of the profile "Standard"
// shows the files of this device (off from the start) and each cloud storage (asks until decided).
// A change saves on selection, and a change from elsewhere shows at once. Needs Docker for RustFS;
// skipped without it.

const BUCKET = 'holzi-agent-files-e2e'

const textOf = (instance: FlowInstance, hook: string) =>
  instance.exec<string>(
    `const el = document.querySelector('[data-testid="' + arguments[0] + '"]')
     return el ? el.textContent.trim() : ''`,
    [hook],
  )

const checked = (instance: FlowInstance, hook: string) =>
  instance.exec<string | null>(
    `return document.querySelector('[data-testid="${hook}"]').getAttribute('aria-checked')`,
  )

scenario(
  'settings-agent-files',
  { needs: { container: true } },
  async (ctx) => {
    const rustfs = await startRustfs([BUCKET])
    ctx.onTeardown(() => rustfs.stop())
    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-agent-files' })
    const storageId = await connectStorage(instance, rustfs, BUCKET, 'Fotos')
    const device = 'mcp-files-device'
    const storage = `mcp-files-storage-${storageId}`

    await openStandardProfile(instance)
    await instance.waitForDisplayed(storage)
    assert.equal(await checked(instance, device), 'false')
    assert.match(await textOf(instance, storage), /Fragt nach|Asks/)
    assert.deepEqual((await standardRights(instance)).files ?? {}, {})
    ctx.step('the device is off, the storage asks')

    await instance.click(device)
    await ctx.waitFor(
      'the device to be granted',
      async () => (await standardRights(instance)).files?.device === true,
    )
    await choose(instance, storage, 'read')
    await ctx.waitFor(
      'the storage to be readable',
      async () =>
        (await standardRights(instance)).files?.storages?.[storageId] ===
        'read',
    )
    ctx.step('saved on selection')

    await editStandardRights(instance, {
      files: {
        device: false,
        storages: { [storageId]: 'readWrite' },
      },
    })
    await ctx.waitFor('the change from elsewhere to show', async () =>
      /Lesen und Schreiben|Read and write/.test(
        await textOf(instance, storage),
      ),
    )
    assert.equal(await checked(instance, device), 'false')
    ctx.step('a change from elsewhere shows at once')
  },
)
