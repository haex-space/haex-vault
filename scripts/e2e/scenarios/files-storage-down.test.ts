import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { createAndUnlock } from '../lib/flows.ts'
import { contextMenu } from '../lib/passwords.ts'
import { putObject, startRustfs } from '../lib/rustfs.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'
import { connectStorage, filesGo } from '../lib/storage-setup.ts'

// Spec 044, US5, quickstart §5 (T065): a folder of a storage comes down onto the device through
// haex-vault's clipboard, its tree kept, written through part files. Needs Docker for RustFS; skipped
// without it.

const BUCKET = 'haex-files-down'

scenario(
  'files-storage-down',
  { timeoutMs: 240_000, needs: { container: true } },
  async (ctx) => {
    const rustfs = await startRustfs([BUCKET])
    ctx.onTeardown(() => rustfs.stop())
    await putObject(rustfs, BUCKET, 'Fotos/a.txt', 'a')
    await putObject(rustfs, BUCKET, 'Fotos/tief/b.txt', 'b')

    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-storage-down' })
    const device = deviceFiles('haex-down-')
    ctx.onTeardown(() => device.remove())
    device.write('schon-da.txt', 'x')
    const storageId = await connectStorage(instance, rustfs, BUCKET, 'Ablage')

    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/storage/${storageId}?p=${encodeURIComponent('/')}`,
    })
    await instance.waitForDisplayed('files-entry-Fotos')
    await contextMenu(instance, 'files-entry-Fotos')
    await instance.click('files-menu-copy')
    await filesGo(instance, `/device?p=${encodeURIComponent(device.folder)}`)
    await instance.waitForDisplayed('files-entry-schon-da.txt')
    await instance.click('files-action-paste')
    await instance.waitForDisplayed('files-entry-Fotos')
    await ctx.waitFor('the tree to arrive', async () => {
      try {
        return device.read('Fotos/tief/b.txt') === 'b'
      } catch {
        return false
      }
    })
    assert.equal(device.read('Fotos/a.txt'), 'a')
    ctx.step('a folder came down')
  },
)
