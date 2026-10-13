import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { unwrap } from '../lib/flows.ts'
import { deviceFiles } from '../lib/extension-files.ts'
import {
  fixture,
  inFrame,
  install,
  openFromLauncher,
  probeRequest,
  type BridgeAnswer,
  type InstalledExtension,
} from '../lib/extensions.ts'
import type { Page } from '../lib/page.ts'

/** Starts a bridge request in the probe without waiting; `answer` reads its answer later. */
async function begin(
  page: Page,
  probe: InstalledExtension,
  method: string,
  params: unknown,
): Promise<void> {
  await inFrame(
    page,
    probe,
    `window.answer = window.probe.request(arguments[0], arguments[1])
       .then((a) => JSON.parse(JSON.stringify(a)))`,
    [method, params],
  )
}

function answer(page: Page, probe: InstalledExtension): Promise<BridgeAnswer> {
  return inFrame<BridgeAnswer>(page, probe, `return window.answer`)
}

function shown(page: Page, testid: string): Promise<boolean> {
  return page.exec<boolean>(
    `return !!document.querySelector('[data-testid="' + arguments[0] + '"]')`,
    [testid],
  )
}

function text(page: Page, testid: string): Promise<string> {
  return page.exec<string>(
    `return document.querySelector('[data-testid="' + arguments[0] + '"]')?.textContent ?? ''`,
    [testid],
  )
}

// Spec 048 user story 7, quickstart §10 step 5 (T081): an encrypted folder on the device through the
// SDK's `encryptedFolders` with the storage id `device:/`. Without file access to the folder that
// holds it the extension is refused and nobody is asked; with it, holzi's chooser offers the folder
// "on this device" and asks; the round trip is decrypted for the extension and encrypted on the
// disk; the raw directory stays closed to the extension's file access and out of its listings.
scenario('extension-encrypted-device', { timeoutMs: 300_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop'] } })
  const page = group.device('anna/laptop').page
  const device = deviceFiles('holzi-ext-encrypted-device-')
  ctx.onTeardown(() => device.remove())
  device.write('offen.txt', 'offen')

  unwrap(
    'files_create_encrypted_folder',
    await page.invoke('files_create_encrypted_folder', {
      source: { kind: 'device' },
      path: device.folder,
      name: 'Tresor',
    }),
  )
  const raw = (device.files().find((file) => file.endsWith('.hxef/h')) ?? '')
    .split('/')
    .at(0)
  assert.match(raw ?? '', /^[a-z2-7]{26}\.hxef$/)
  const probe = await install(page, fixture('e2e', 'probe'))
  await openFromLauncher(page, probe)
  await ctx.waitFor(
    'the SDK channel of the probe',
    () =>
      inFrame<boolean>(
        page,
        probe,
        `return document.documentElement.dataset.probeReady === '1'`,
      ).catch(() => false),
    { timeoutMs: 20_000 },
  )
  ctx.step('an encrypted folder "Tresor" in a device folder')

  const choose = { request: { storageId: 'device:/', access: 'readWrite' } }
  const without = await probeRequest(
    page,
    probe,
    'extension_encrypted_folder_choose',
    choose,
  )
  assert.equal(without.error?.code, 1002, JSON.stringify(without))
  assert.ok(!(await shown(page, 'extension-encrypted-chooser')))
  assert.ok(!(await shown(page, 'extension-permission-request')))
  ctx.step('without file access to its place: refused, nobody asked')

  unwrap(
    'extension_permission_set',
    await page.invoke('extension_permission_set', {
      args: {
        extensionId: probe.id,
        kind: 'filesystem',
        action: 'readWrite',
        target: device.folder,
        status: 'granted',
      },
    }),
  )
  await begin(page, probe, 'extension_encrypted_folder_choose', choose)
  await page.waitForDisplayed(
    'extension-encrypted-chooser-folder-Tresor',
    10_000,
  )
  assert.match(
    await text(page, 'extension-encrypted-chooser'),
    /diesem Gerät|this device/,
    'the chooser says the folder is on this device',
  )
  await page.click('extension-encrypted-chooser-read-write')
  const chosen = await answer(page, probe)
  assert.equal(chosen.error, undefined, JSON.stringify(chosen))
  const { folderId, name } = chosen.result as {
    folderId: string
    name: string
  }
  assert.equal(name, 'Tresor')
  ctx.step('with file access the chooser offers the folder and asks')

  const at = (path: string) => ({
    request: { storageId: 'device:/', folderId, path },
  })
  const put = await probeRequest(
    page,
    probe,
    'extension_encrypted_folder_upload',
    {
      request: {
        ...at('notiz.txt').request,
        data: Buffer.from('hallo vom Gerät').toString('base64'),
      },
    },
  )
  assert.equal(put.error, undefined, JSON.stringify(put))
  const got = await probeRequest(
    page,
    probe,
    'extension_encrypted_folder_download',
    at('notiz.txt'),
  )
  assert.equal(
    Buffer.from(String(got.result), 'base64').toString(),
    'hallo vom Gerät',
  )
  const listed = await probeRequest(
    page,
    probe,
    'extension_encrypted_folder_list',
    at(''),
  )
  assert.deepEqual(
    (listed.result as { entries: { name: string }[] }).entries.map(
      (entry) => entry.name,
    ),
    ['notiz.txt'],
  )
  for (const file of device.files(raw)) {
    assert.match(file, /\.hxef\/(h|[mc]\/[a-z2-7]{26})$/, file)
    assert.ok(!device.readBytes(file).includes('hallo vom Gerät'), file)
  }
  ctx.step('decrypted round trip; the disk holds only encrypted objects')

  const rawRead = await probeRequest(
    page,
    probe,
    'extension_filesystem_read_file',
    { path: `${device.folder}/${raw}/h` },
  )
  assert.equal(rawRead.error?.code, 1002, JSON.stringify(rawRead))
  const folder = await probeRequest(
    page,
    probe,
    'extension_filesystem_read_dir',
    { path: device.folder },
  )
  assert.deepEqual(
    (folder.result as { name: string }[]).map((entry) => entry.name),
    ['offen.txt'],
  )
  ctx.step('the raw directory is closed to file access and out of listings')
})
