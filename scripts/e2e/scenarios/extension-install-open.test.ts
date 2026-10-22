import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { setSessionRestore } from '../lib/settings.ts'
import {
  backInTab,
  fixture,
  frameOf,
  inFrame,
  install,
  openFromLauncher,
  probeRequest,
  refusal,
  start,
} from '../lib/extensions.ts'

// Spec 017, US1, T051 (quickstart §2): manipulated and pre-v2 bundles are refused; a signed bundle
// installs, opens from the launcher in its sandboxed frame (its module script runs under the CSP),
// talks to holzi over the SDK channel, navigates inside its tab with holzi's Back, and comes back
// with the session after a restart.
scenario('extension-install-open', { timeoutMs: 300_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop'] } })
  const device = group.device('anna/laptop')
  const page = device.page
  await setSessionRestore(page, true)

  assert.equal(
    await refusal(page, fixture('vectors', 'bad-moved-content')),
    'ExtensionInstall:file_mismatch',
  )
  assert.equal(
    await refusal(page, fixture('vectors', 'legacy-format')),
    'ExtensionInstall:legacy_signature_format',
  )
  ctx.step('manipulated and pre-v2 bundles refused')

  // A bundle without the SDK still starts on this device (its migrations run).
  const notes = await install(page, fixture('vectors', 'good-notes-like'))
  await start(page, notes)
  ctx.step('notes installed and started')

  const probe = await install(page, fixture('e2e', 'probe'))
  await openFromLauncher(page, probe)
  await ctx.waitFor(
    'the SDK channel of the probe',
    () =>
      inFrame<boolean>(
        page,
        probe,
        `return document.documentElement.dataset.probeReady === '1'`,
      ),
    { timeoutMs: 20_000 },
  )
  assert.equal(
    await inFrame<string>(
      page,
      probe,
      `return document.getElementById('module').textContent`,
    ),
    'module ran',
    'the module script of the bundle runs under the CSP',
  )
  const context = await probeRequest(page, probe, 'extension_context_get', {})
  assert.ok(
    typeof (context.result as { deviceId?: unknown })?.deviceId === 'string',
    JSON.stringify(context),
  )
  const unsupported = await probeRequest(
    page,
    probe,
    'extension_space_list',
    {},
  )
  assert.equal(unsupported.error?.code, 8000)
  ctx.step('SDK channel answers, unsupported methods say 8000')

  await inFrame(
    page,
    probe,
    `document.getElementById('to-second').click(); return true`,
  )
  await ctx.waitFor(
    'holzi to record the place inside the extension',
    async () =>
      JSON.stringify(await page.invoke('wm_session_load')).includes('/second'),
    { fixed: true, timeoutMs: 15_000 },
  )
  await backInTab(page, probe)
  await ctx.waitFor(
    'the frame to follow holzi Back',
    async () =>
      (await inFrame<string>(
        page,
        probe,
        `return document.getElementById('place').textContent`,
      )) === '#/',
    { timeoutMs: 10_000 },
  )
  ctx.step('navigation inside the tab and holzi Back')

  await device.restart()
  const restored = device.page
  await restored.waitForDisplayed(frameOf(probe.id), 30_000)
  ctx.step('tab restored after a restart')
})
