import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { unwrap, type FlowInstance } from '../lib/flows.ts'
import type { Page } from '../lib/page.ts'
import {
  KEY,
  openSettings,
  runAction,
  settingsTabs,
  wmSnapshot,
} from '../lib/settings.ts'
import {
  fixture,
  inFrame,
  install,
  openFromLauncher,
  probeRequest,
  type InstalledExtension,
} from '../lib/extensions.ts'
import { errorToasts } from '../lib/toasts.ts'

// Spec 017, US7, T090 (quickstart §7 fourth bullet), through the settings as the user does it:
// disabling closes the extension's tab and takes it out of the launcher, enabling brings it back;
// removing with "keep data" lists the kept data, and installing again finds the rows; deleting
// the kept data leaves nothing listed.

interface Listed {
  id: string
  state: string
  enabled: boolean
}

async function listed(
  page: Page & FlowInstance,
  extension: InstalledExtension,
): Promise<Listed | undefined> {
  return unwrap<Listed[]>(
    'extension_list',
    await page.invoke('extension_list'),
  ).find((e) => e.id === extension.id)
}

async function hasTab(
  page: Page & FlowInstance,
  extension: InstalledExtension,
) {
  const snapshot = await wmSnapshot(page)
  return snapshot.windows.some((w) =>
    w.tabs.some((t) => t.appId === `extension.${extension.id}`),
  )
}

/** Whether the extension is one of the apps the launcher offers. */
async function inLauncher(
  page: Page & FlowInstance,
  extension: InstalledExtension,
) {
  const apps = await runAction(page, 'wm.apps.list')
  assert.ok(apps.ok, JSON.stringify(apps))
  return JSON.stringify(apps.result).includes(`extension.${extension.id}`)
}

/** Brings the settings window, still at the extension, to the front. */
async function focusSettings(page: Page & FlowInstance) {
  const [settings] = settingsTabs(await wmSnapshot(page))
  assert.ok(settings, 'the settings are open')
  const focused = await runAction(page, 'wm.window.focus', {
    windowId: settings.windowId,
  })
  assert.ok(focused.ok, JSON.stringify(focused))
}

async function openDetail(
  page: Page & FlowInstance,
  extension: InstalledExtension,
) {
  await openSettings(page)
  await page.click('settings-category-extensions')
  await page.click(
    `[data-testid="extension-row"][data-extension-id="${extension.id}"]`,
  )
}

async function maxRows(
  page: Page & FlowInstance,
  extension: InstalledExtension,
): Promise<number> {
  return unwrap<{ values: { maxRows: number } }>(
    'extension_limits_get',
    await page.invoke('extension_limits_get', { extensionId: extension.id }),
  ).values.maxRows
}

async function rowsField(page: Page & FlowInstance): Promise<string> {
  return page.exec<string>(
    `return document.querySelector('[data-testid="extension-limit-maxRows"]')?.value ?? ''`,
  )
}

async function waitReady(
  page: Page & FlowInstance,
  extension: InstalledExtension,
) {
  return inFrame<boolean>(
    page,
    extension,
    `return document.documentElement.dataset.probeReady === '1'`,
  ).catch(() => false)
}

scenario('extension-lifecycle', { timeoutMs: 300_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop'] } })
  const page = group.device('anna/laptop').page

  const probe = await install(page, fixture('e2e', 'probe'))
  await openFromLauncher(page, probe)
  await ctx.waitFor('the probe to be ready', () => waitReady(page, probe), {
    timeoutMs: 20_000,
  })
  const info = await probeRequest(page, probe, 'extension_get_info', {})
  const table = `${(info.result as { publicKey: string }).publicKey}__probe__items`
  const written = await probeRequest(
    page,
    probe,
    'extension_database_execute',
    {
      query: `INSERT INTO "${table}" (id, label) VALUES ('kept', 'kept')`,
      params: [],
    },
  )
  assert.equal(written.error, undefined, JSON.stringify(written))
  ctx.step('installed, opened and written')

  await openDetail(page, probe)
  await ctx.waitFor(
    'the limits to show',
    async () => (await rowsField(page)) === '10000',
    { timeoutMs: 10_000 },
  )
  // An emptied field is no limit: the stored one comes back, nothing is sent.
  await page.type(
    'extension-limit-maxRows',
    KEY.backspace.repeat(5) + KEY.enter,
  )
  await ctx.waitFor(
    'the stored limit to come back',
    async () => (await rowsField(page)) === '10000',
    { timeoutMs: 5_000 },
  )
  assert.equal(await maxRows(page, probe), 10_000)
  assert.equal(
    (await errorToasts(page)).length,
    0,
    'no refusal shown for an emptied field',
  )
  // A value outside the bounds is refused with the reason, and the stored one comes back.
  await page.type(
    'extension-limit-maxRows',
    KEY.backspace.repeat(5) + '0' + KEY.enter,
  )
  await ctx.waitFor(
    'the refusal to show',
    async () => (await errorToasts(page)).length === 1,
    { timeoutMs: 5_000 },
  )
  // The refusal shows before the stored value is read back into the field.
  await ctx.waitFor(
    'the stored limit to come back after the refusal',
    async () => (await rowsField(page)) === '10000',
    { timeoutMs: 5_000 },
  )
  await page.type(
    'extension-limit-maxRows',
    KEY.backspace.repeat(5) + '500' + KEY.enter,
  )
  await ctx.waitFor(
    'the new limit to be stored',
    async () => (await maxRows(page, probe)) === 500,
    { timeoutMs: 5_000 },
  )
  ctx.step('limits: an emptied field restores, a new value is stored')
  await page.click('extension-enabled')
  await ctx.waitFor(
    'the tab of the disabled extension to close',
    async () => !(await hasTab(page, probe)),
    { timeoutMs: 10_000 },
  )
  assert.equal((await listed(page, probe))?.enabled, false)
  await ctx.waitFor(
    'the extension gone from the launcher',
    async () => !(await inLauncher(page, probe)),
    { timeoutMs: 10_000 },
  )
  ctx.step('disabled: its tab closed, not in the launcher')

  await page.click('extension-enabled')
  await ctx.waitFor(
    'the extension to be enabled again',
    async () => (await listed(page, probe))?.enabled === true,
    { timeoutMs: 10_000 },
  )
  // The launcher reloads its list after holzi stored the change.
  await ctx.waitFor(
    'the extension in the launcher again',
    () => inLauncher(page, probe),
    { timeoutMs: 10_000 },
  )
  await page.click('extension-remove')
  await page.click('extension-remove-keep')
  await ctx.waitFor(
    'the kept data to be listed',
    async () => (await listed(page, probe))?.state === 'removed',
    { timeoutMs: 10_000 },
  )
  await page.waitForDisplayed('extension-purge-kept-data', 10_000)
  ctx.step('enabled again, then removed with "keep data"')

  await install(page, fixture('e2e', 'probe'))
  await openFromLauncher(page, probe)
  await ctx.waitFor('the probe to be ready', () => waitReady(page, probe), {
    timeoutMs: 20_000,
  })
  const found = await probeRequest(page, probe, 'extension_database_query', {
    query: `SELECT id FROM "${table}"`,
    params: [],
  })
  assert.deepEqual(
    (found.result as { rows?: unknown[][] } | undefined)?.rows,
    [['kept']],
    'a new install finds the kept rows',
  )
  ctx.step('installed again: the kept rows are there')

  // The settings still show the extension, now installed again.
  await focusSettings(page)
  await page.click('extension-remove')
  await page.click('extension-remove-keep')
  await ctx.waitFor(
    'the tab of the removed extension to close',
    async () => !(await hasTab(page, probe)),
    { timeoutMs: 10_000 },
  )
  await page.click('extension-purge-kept-data')
  await page.click('extension-purge-kept-data-confirm')
  await ctx.waitFor(
    'nothing of the extension to be listed',
    async () => (await listed(page, probe)) === undefined,
    { timeoutMs: 10_000 },
  )
  ctx.step('kept data deleted: nothing listed')
})
