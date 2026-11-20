import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { unwrap, type FlowInstance } from '../lib/flows.ts'
import type { Page } from '../lib/page.ts'
import { runAction, setSessionRestore } from '../lib/settings.ts'
import {
  fixture,
  inFrame,
  install,
  openFromLauncher,
  probeRequest,
  type InstalledExtension,
} from '../lib/extensions.ts'

// Spec 017, US5, T082 (quickstart §7 first two bullets): a write to its table reaches the open
// probe as `haextension:sync:tables-updated` without the name of any core table; switching the
// color scheme reaches it as `haextension:context:changed`; a stored value survives a restart; a
// log entry is read back by the extension and by haex-vault's settings.

interface FrameEvent {
  type: string
  data: { tables?: string[]; context?: { theme?: string } }
}

type Instance = Page & FlowInstance

async function events(page: Instance, probe: InstalledExtension) {
  return inFrame<FrameEvent[]>(
    page,
    probe,
    `return JSON.parse(JSON.stringify(window.probe.events))`,
  )
}

async function waitReady(page: Instance, probe: InstalledExtension) {
  return inFrame<boolean>(
    page,
    probe,
    `return document.documentElement.dataset.probeReady === '1'`,
  ).catch(() => false)
}

scenario('extension-notifications', { timeoutMs: 300_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop'] } })
  const device = group.device('anna/laptop')
  const page = device.page
  await setSessionRestore(page, true)

  const probe = await install(page, fixture('e2e', 'probe'))
  await openFromLauncher(page, probe)
  await ctx.waitFor('the probe to be ready', () => waitReady(page, probe), {
    timeoutMs: 20_000,
  })
  const info = await probeRequest(page, probe, 'extension_get_info', {})
  const publicKey = (info.result as { publicKey: string }).publicKey
  const table = `${publicKey}__probe__items`

  const written = await probeRequest(
    page,
    probe,
    'extension_database_execute',
    {
      query: `INSERT INTO "${table}" (id, label) VALUES ('n1', 'note')`,
      params: [],
    },
  )
  assert.equal(written.error, undefined, JSON.stringify(written))
  await ctx.waitFor(
    'the probe to hear of its table',
    async () =>
      (await events(page, probe)).some(
        (e) =>
          e.type === 'haextension:sync:tables-updated' &&
          (e.data.tables ?? []).includes(table),
      ),
    { timeoutMs: 10_000 },
  )
  const named = (await events(page, probe))
    .filter((e) => e.type === 'haextension:sync:tables-updated')
    .flatMap((e) => e.data.tables ?? [])
  assert.ok(
    named.every((name) => name.startsWith(`${publicKey}__probe__`)),
    `only its own tables: ${JSON.stringify(named)}`,
  )
  ctx.step('a write reaches the probe, without core tables')

  const switched = await runAction(page, 'settings.appearance.setColorScheme', {
    scheme: 'dark',
  })
  assert.ok(switched.ok, JSON.stringify(switched))
  await ctx.waitFor(
    'the probe to hear of the new context',
    async () =>
      (await events(page, probe)).some(
        (e) =>
          e.type === 'haextension:context:changed' &&
          e.data.context?.theme === 'dark',
      ),
    { timeoutMs: 10_000 },
  )
  ctx.step('the color scheme reaches the probe')

  const stored = await probeRequest(
    page,
    probe,
    'extension_web_storage_set_item',
    { key: 'zoom', value: '2' },
  )
  assert.equal(stored.error, undefined, JSON.stringify(stored))
  const logged = await probeRequest(page, probe, 'extension_logging_write', {
    level: 'warn',
    message: 'probe says hi',
  })
  assert.equal(logged.error, undefined, JSON.stringify(logged))
  const own = await probeRequest(page, probe, 'extension_logging_read', {})
  assert.equal(
    (own.result as { message: string }[] | undefined)?.[0]?.message,
    'probe says hi',
  )

  await device.restart()
  const restored = device.page
  await ctx.waitFor(
    'the restored probe to be ready',
    () => waitReady(restored, probe),
    { timeoutMs: 30_000 },
  )
  const value = await probeRequest(
    restored,
    probe,
    'extension_web_storage_get_item',
    { key: 'zoom' },
  )
  assert.equal(value.result, '2', 'the value survives a restart')
  const inSettings = unwrap<{ message: string }[]>(
    'extension_logs_read',
    await restored.invoke('extension_logs_read', {
      extensionId: probe.id,
      level: null,
      limit: 10,
      before: null,
    }),
  )
  assert.equal(inSettings[0]?.message, 'probe says hi')
  ctx.step('stored value and log entry after a restart')
})
