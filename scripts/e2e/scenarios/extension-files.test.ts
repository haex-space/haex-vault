import assert from 'node:assert/strict'
import { deviceFiles } from '../lib/extension-files.ts'
import { scenario } from '../lib/scenario.ts'
import type { FlowInstance } from '../lib/flows.ts'
import type { Page } from '../lib/page.ts'
import {
  fixture,
  inFrame,
  install,
  openFromLauncher,
  probeRequest,
  type InstalledExtension,
} from '../lib/extensions.ts'

// Spec 017, US9, T101 (quickstart §8): a file of the device asks the user before the probe reads
// it, writing asks again, and a watched folder reports a change to the probe in the SDK's flat
// form. Dialog choices, haex-vault's protected places and links are covered by the lib tests: native
// dialogs cannot be driven here.

type Instance = Page & FlowInstance

interface ProbeEvent {
  type: string
  ruleId?: string
  changeType?: string
  path?: string
}

async function allow(page: Instance) {
  await page.waitForDisplayed('extension-permission-request', 10_000)
  await page.click('extension-permission-allow')
}

async function events(page: Instance, probe: InstalledExtension) {
  return inFrame<ProbeEvent[]>(
    page,
    probe,
    `return JSON.parse(JSON.stringify(window.probe.events))`,
  )
}

scenario('extension-files', { timeoutMs: 300_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop'] } })
  const page = group.device('anna/laptop').page
  const files = deviceFiles()
  const note = files.path('note.txt')
  files.write('note.txt', 'from the device')

  const probe = await install(page, fixture('e2e', 'probe'))
  await openFromLauncher(page, probe)
  await ctx.waitFor(
    'the probe to be ready',
    () =>
      inFrame<boolean>(
        page,
        probe,
        `return document.documentElement.dataset.probeReady === '1'`,
      ).catch(() => false),
    { timeoutMs: 20_000 },
  )

  const read = () =>
    probeRequest(page, probe, 'extension_filesystem_read_file', { path: note })
  assert.equal((await read()).error?.code, 1004)
  await allow(page)
  const content = (await read()).result as string
  assert.equal(Buffer.from(content, 'base64').toString(), 'from the device')
  ctx.step('reading asked first, then the file came')

  const target = files.path('written.txt')
  const write = () =>
    probeRequest(page, probe, 'extension_filesystem_write_file', {
      path: target,
      data: Buffer.from('from the probe').toString('base64'),
    })
  assert.equal((await write()).error?.code, 1004, 'writing is asked for')
  await allow(page)
  assert.equal((await write()).error, undefined)
  assert.equal(files.read('written.txt'), 'from the probe')
  ctx.step('writing asked again, then the file was written')

  const watch = () =>
    probeRequest(page, probe, 'extension_filesystem_watch', {
      ruleId: 'notes',
      path: files.folder,
    })
  assert.equal((await watch()).error?.code, 1004)
  await allow(page)
  assert.equal((await watch()).error, undefined)
  files.write('changed.txt', 'x')
  await ctx.waitFor(
    'the change to reach the probe',
    async () =>
      (await events(page, probe)).some(
        (e) =>
          e.type === 'filesync:file-changed' &&
          e.ruleId === 'notes' &&
          e.path === 'changed.txt',
      ),
    { timeoutMs: 15_000 },
  )
  ctx.step('a watched folder reports to the probe, flat')
  files.remove()
})
