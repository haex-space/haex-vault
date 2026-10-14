import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { createAndUnlock, unwrap, type FlowInstance } from '../lib/flows.ts'
import { contextMenu, doubleClick } from '../lib/passwords.ts'
import { scenario, type ScenarioContext } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'
import { prepareSftp } from '../lib/sftp.ts'
import { connectSftp, connectWebdav, filesGo } from '../lib/storage-setup.ts'
import { listTree, putFile, startWebdav } from '../lib/webdav.ts'

// Spec 050 US7 (T090): an encrypted folder of spec 048 in a WebDAV and in an SFTP storage, used
// through the file browser as on S3. A device file is pasted in and opens from the folder; the
// server holds only the random directory with `h`, `m/…` and `c/…`, no name and nothing in the
// clear; deleting the folder asks first and leaves nothing of it. Needs Docker; skipped without.

const FOLDER = 'Ablage'
const NOTE = 'Hallo aus dem verschlüsselten Ordner'
const OBJECT = /^[a-z2-7]{26}\.hxef\/(|h|[mc]\/|[mc]\/[a-z2-7]{26})$/

const gone = (instance: FlowInstance, hook: string) =>
  instance.exec<boolean>(
    `return !document.querySelector('[data-testid="' + arguments[0] + '"]')`,
    [hook],
  )

const textOf = (instance: FlowInstance, hook: string) =>
  instance.exec<string>(
    `const el = document.querySelector('[data-testid="' + arguments[0] + '"]')
     return el ? el.textContent : ''`,
    [hook],
  )

/** The encrypted folder "Tresor" in the storage `storageId`, used as on S3; `tree` lists the
 * storage's folder on the server. */
async function useEncrypted(
  ctx: ScenarioContext,
  instance: FlowInstance,
  storageId: string,
  deviceFolder: string,
  tree: () => Promise<string[]>,
  protocol: string,
) {
  unwrap(
    'files_create_encrypted_folder',
    await instance.invoke('files_create_encrypted_folder', {
      source: { kind: 'storage', storageId },
      path: '/',
      name: 'Tresor',
    }),
  )
  const storageAt = `/storage/${storageId}?p=${encodeURIComponent('/')}`
  await filesGo(instance, `/device?p=${encodeURIComponent(deviceFolder)}`)
  await instance.waitForDisplayed('files-entry-notiz.txt')
  await contextMenu(instance, 'files-entry-notiz.txt')
  await instance.click('files-menu-copy')
  await filesGo(instance, storageAt)
  await instance.waitForDisplayed('files-entry-Tresor', 15_000)
  await doubleClick(instance, 'files-entry-Tresor')
  await instance.click('files-action-paste')
  await instance.waitForDisplayed('files-entry-notiz.txt', 30_000)
  await doubleClick(instance, 'files-entry-notiz.txt')
  await instance.waitForDisplayed('files-viewer-text')
  assert.match(await textOf(instance, 'files-viewer-text'), new RegExp(NOTE))
  await instance.click('files-viewer-close')
  ctx.step(`${protocol}: a file went in and opens from the encrypted folder`)

  const stored = (await tree()).filter((entry) => entry.includes('.hxef'))
  assert.ok(stored.length > 0, `${protocol}: ${stored}`)
  for (const entry of stored) {
    assert.match(entry, OBJECT, `${protocol}: ${entry}`)
    assert.ok(!/Tresor|notiz/.test(entry), entry)
  }
  ctx.step(`${protocol}: the server holds only the format, no names`)

  await filesGo(instance, storageAt)
  await instance.waitForDisplayed('files-entry-Tresor')
  await contextMenu(instance, 'files-entry-Tresor')
  await instance.click('files-menu-delete')
  await instance.click('files-delete-confirm')
  await ctx.waitFor(`${protocol}: the folder to go`, () =>
    gone(instance, 'files-entry-Tresor'),
  )
  await ctx.waitFor(`${protocol}: nothing of it on the server`, async () =>
    (await tree()).every((entry) => !entry.includes('.hxef')),
  )
  ctx.step(`${protocol}: deleting asked and left nothing`)
}

scenario(
  'files-encrypted-tree-storage',
  { timeoutMs: 420_000, needs: { container: true } },
  async (ctx) => {
    const webdav = await startWebdav()
    ctx.onTeardown(() => webdav.stop())
    await putFile(webdav, `${FOLDER}/offen.txt`, 'offen')
    const sftp = await prepareSftp({ password: 'sftp-e2e-password' })
    ctx.onTeardown(() => sftp.stop())
    sftp.writeFile(`${FOLDER}/offen.txt`, 'offen')
    await sftp.startAsync()
    ctx.step('a WebDAV and an SFTP server')

    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-encrypted-tree' })
    const device = deviceFiles('holzi-encrypted-tree-')
    ctx.onTeardown(() => device.remove())
    device.write('notiz.txt', NOTE)
    const dav = await connectWebdav(instance, webdav, FOLDER, 'Server')
    const ssh = await connectSftp(instance, sftp, FOLDER, 'Box')
    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/storage/${dav}?p=${encodeURIComponent('/')}`,
    })
    await instance.waitForDisplayed('files-entry-offen.txt')

    await useEncrypted(
      ctx,
      instance,
      dav,
      device.folder,
      () => listTree(webdav, `/${FOLDER}/`),
      'WebDAV',
    )
    await useEncrypted(
      ctx,
      instance,
      ssh,
      device.folder,
      async () => sftp.listTree(FOLDER),
      'SFTP',
    )
  },
)
