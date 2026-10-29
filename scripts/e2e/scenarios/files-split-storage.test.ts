import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import {
  entryIn,
  inPanel,
  namesIn,
  panelShows,
  selectIn,
  toastTexts,
  visiblePanels,
} from '../lib/files-split.ts'
import { createAndUnlock } from '../lib/flows.ts'
import { doubleClick } from '../lib/passwords.ts'
import { listKeys, putObject, startRustfs } from '../lib/rustfs.ts'
import { scenario } from '../lib/scenario.ts'
import { KEY, runAction } from '../lib/settings.ts'
import { connectStorage } from '../lib/storage-setup.ts'

// Spec 062, US3, quickstart M6, M8: the two panels show different sources. Left the device, right a
// storage of RustFS: F5 uploads a file and a folder, F5 from the storage comes down again, and a
// storage that is down shows its error in its panel while the other panel stays usable. The bucket
// and the device folder are checked directly. Needs Docker for RustFS; skipped without it.

const BUCKET = 'holzi-split-e2e'

scenario(
  'files-split-storage',
  { timeoutMs: 300_000, needs: { container: true } },
  async (ctx) => {
    const rustfs = await startRustfs([BUCKET])
    ctx.onTeardown(() => rustfs.stop())
    await putObject(rustfs, BUCKET, 'Alt/a.txt', 'a')

    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-split-storage' })
    // After the start: on Android it clears the app's data, where the folder lives.
    const device = deviceFiles('holzi-split-storage-')
    ctx.onTeardown(() => device.remove())
    device.write('hoch.txt', 'vom Gerät')
    device.write('ordner/x.txt', 'x')
    const storageId = await connectStorage(instance, rustfs, BUCKET, 'Fotos')
    const press = (keys: string) => instance.typeToFocused(keys)

    // Left the device folder, right the storage's root (`o.src`, `o.p` of the split location).
    const at = new URLSearchParams({
      p: device.folder,
      split: '1',
      'o.src': `storage:${storageId}`,
      'o.p': '/',
    })
    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/device?${at}`,
    })
    await instance.waitForDisplayed('files-sidebar-toggle')
    await instance.click('files-sidebar-toggle')
    await ctx.waitFor(
      'two panels',
      async () => (await visiblePanels(instance)) === 2,
    )
    await instance.waitForDisplayed(entryIn('left', 'hoch.txt'))
    await instance.waitForDisplayed(entryIn('right', 'Alt'))
    ctx.step('the device on the left, the storage on the right')

    // A file and a folder go up.
    await selectIn(ctx, instance, 'left', 'hoch.txt')
    // The keyboard path is covered against device folders in `files-split`; use the explicit
    // action here so this scenario isolates the cross-storage transfer.
    await instance.click(inPanel('left', 'files-bar-copy-other'))
    await ctx
      .waitFor('hoch.txt in the right panel', () =>
        panelShows(instance, 'right', 'hoch.txt'),
      )
      .catch(async (error: Error) => {
        error.message += `; toasts: ${JSON.stringify(await toastTexts(instance))}, bucket: ${JSON.stringify(await listKeys(rustfs, BUCKET))}`
        throw error
      })
    assert.ok((await listKeys(rustfs, BUCKET)).includes('hoch.txt'))
    await selectIn(ctx, instance, 'left', 'ordner')
    await press(KEY.f5)
    await ctx.waitFor('the folder in the bucket', async () =>
      (await listKeys(rustfs, BUCKET)).includes('ordner/x.txt'),
    )
    assert.ok(
      await panelShows(instance, 'left', 'hoch.txt'),
      'a copy leaves the original',
    )
    ctx.step('F5 uploads a file and a folder')

    // And a folder comes down again.
    await selectIn(ctx, instance, 'right', 'Alt')
    await press(KEY.f5)
    await instance.waitForDisplayed(entryIn('left', 'Alt'))
    assert.equal(device.read('Alt/a.txt'), 'a')
    ctx.step('F5 from the storage downloads into the device folder')

    // The storage goes away: its panel says so, the other panel stays usable.
    rustfs.stop()
    await instance.click(inPanel('right', 'files-reload'))
    await instance.waitForDisplayed(inPanel('right', 'files-error'))
    await doubleClick(instance, entryIn('left', 'ordner'))
    await instance.waitForDisplayed(entryIn('left', 'x.txt'))
    assert.deepEqual(await namesIn(instance, 'left'), ['x.txt'])
    ctx.step('a storage that is down fails in its panel only')
  },
)
