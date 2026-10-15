import assert from 'node:assert/strict'
import { createAndUnlock, unwrap, type FlowInstance } from '../lib/flows.ts'
import { startRustfs } from '../lib/rustfs.ts'
import { scenario } from '../lib/scenario.ts'
import { choose, openSettings, waitForLocation } from '../lib/settings.ts'
import { connectStorage } from '../lib/storage-setup.ts'

// Spec 044, T077 (FR-031b): Einstellungen → Agents → Dateien shows the file permissions of the agent
// in the chat: the files of this device (granted from the start) and each cloud storage (asks until
// decided). A change saves on selection, and a change from elsewhere shows at once. Needs Docker
// for RustFS; skipped without it.

const BUCKET = 'holzi-agent-files-e2e'

type Row = { agentId: string; kind: string; target: string; status: string }

const rows = async (instance: FlowInstance) =>
  unwrap<Row[]>(
    'files_agent_permissions',
    await instance.invoke('files_agent_permissions'),
  )

const textOf = (instance: FlowInstance, hook: string) =>
  instance.exec<string>(
    `const el = document.querySelector('[data-testid="' + arguments[0] + '"]')
     return el ? el.textContent.trim() : ''`,
    [hook],
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
    const device = 'agent-files-device-builtin'
    const storage = `agent-files-storage-builtin-${storageId}`

    await openSettings(instance)
    await instance.click('settings-category-ai')
    await waitForLocation(instance, 'ai')
    await instance.click('settings-row-ai.files')
    await waitForLocation(instance, 'ai.files')
    await instance.waitForDisplayed(storage)
    assert.equal(
      await instance.exec<string | null>(
        `return document.querySelector('[data-testid="${device}"]').getAttribute('aria-checked')`,
      ),
      'true',
    )
    assert.match(await textOf(instance, storage), /Fragt nach|Asks first/)
    assert.deepEqual(await rows(instance), [])
    ctx.step('the device granted, the storage asks')

    await instance.click(device)
    await ctx.waitFor('the device to be revoked', async () =>
      (await rows(instance)).some(
        (row) => row.kind === 'device' && row.status === 'denied',
      ),
    )
    await choose(instance, storage, 'read')
    await ctx.waitFor('the storage to be readable', async () =>
      (await rows(instance)).some(
        (row) =>
          row.kind === 'storage' &&
          row.target === storageId &&
          row.status === 'read',
      ),
    )
    ctx.step('saved on selection')

    unwrap(
      'files_agent_permission_set',
      await instance.invoke('files_agent_permission_set', {
        agentId: 'builtin',
        kind: 'storage',
        target: storageId,
        status: 'readWrite',
      }),
    )
    await ctx.waitFor('the change from elsewhere to show', async () =>
      /Lesen und Schreiben|Read and write/.test(
        await textOf(instance, storage),
      ),
    )
    assert.equal(
      await instance.exec<string | null>(
        `return document.querySelector('[data-testid="${device}"]').getAttribute('aria-checked')`,
      ),
      'false',
    )
    ctx.step('a change from elsewhere shows at once')
  },
)
