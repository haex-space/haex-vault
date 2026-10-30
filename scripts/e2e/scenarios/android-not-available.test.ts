import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import {
  fixture,
  inFrame,
  install,
  openFromLauncher,
  probeRequest,
} from '../lib/extensions.ts'
import { deviceFiles } from '../lib/extension-files.ts'
import { runAction, waitForLocation } from '../lib/settings.ts'

/** The bridge's answer for what this device does not have (spec 017, FR-066). */
const NOT_AVAILABLE = 8001
/** The bridge's answer for a refused permission. */
const DENIED = 1002

// Spec 043 FR-016, FR-026 (the counter case of `extension-files`, `extension-dev-mode` and
// `extension-encrypted-device`): what a phone does not have is not offered and says so. The
// capability table names it; an extension that reads a free path, picks a folder or watches one
// hears "not available", and an encrypted folder on the device stays closed to it; developer mode
// (a project folder) is left out of the extension settings, and the delegate settings, the
// access for external agents (`mcp-agent`) and the agent browser (spec 065) say "not available on
// this device".
scenario('android-not-available', { needs: { phone: true } }, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'phone-vault' })
  const table = unwrap<Record<string, unknown>>(
    'platform_capabilities',
    await instance.invoke('platform_capabilities'),
  )
  for (const facility of [
    'cliDelegates',
    'commandTool',
    'terminal',
    'folderWatch',
    'freePaths',
    'folderPick',
    // Spec 049 FR-043: no External Bridge for browser extensions.
    'browserBridge',
  ]) {
    assert.equal(table[facility], false, facility)
  }
  ctx.step('the capability table names what is missing')

  const probe = await install(instance, fixture('e2e', 'probe'))
  await openFromLauncher(instance, probe)
  await ctx.waitFor(
    'the probe to be ready',
    () =>
      inFrame<boolean>(
        instance,
        probe,
        `return document.documentElement.dataset.probeReady === '1'`,
      ).catch(() => false),
    { timeoutMs: 20_000 },
  )
  const answers = {
    'a free path': await probeRequest(
      instance,
      probe,
      'extension_filesystem_read_file',
      { path: '/system/etc/hosts' },
    ),
    'a folder dialog': await probeRequest(
      instance,
      probe,
      'extension_filesystem_select_folder',
      {},
    ),
    'a watched folder': await probeRequest(
      instance,
      probe,
      'extension_filesystem_watch',
      { ruleId: 'notes', path: '/system/etc' },
    ),
  }
  for (const [what, answer] of Object.entries(answers)) {
    assert.equal(answer.error?.code, NOT_AVAILABLE, what)
  }
  ctx.step('an extension hears "not available" for free paths and folders')

  // The counter case of `extension-encrypted-device`: an extension reaches an encrypted folder on
  // the device only with file access to its place (spec 048 US7), and a phone has no free paths, so
  // even a stored permission there leaves it refused and nobody asked.
  const device = deviceFiles('holzi-not-available-')
  ctx.onTeardown(() => device.remove())
  device.write('offen.txt', 'offen')
  unwrap(
    'files_create_encrypted_folder',
    await instance.invoke('files_create_encrypted_folder', {
      source: { kind: 'device' },
      path: device.folder,
      name: 'Tresor',
    }),
  )
  unwrap(
    'extension_permission_set',
    await instance.invoke('extension_permission_set', {
      args: {
        extensionId: probe.id,
        kind: 'filesystem',
        action: 'readWrite',
        target: device.folder,
        status: 'granted',
      },
    }),
  )
  const chooser = await probeRequest(
    instance,
    probe,
    'extension_encrypted_folder_choose',
    { request: { storageId: 'device:/', access: 'readWrite' } },
  )
  assert.equal(chooser.error?.code, DENIED, JSON.stringify(chooser))
  assert.equal(
    await instance.exec<boolean>(
      `return document.querySelector('[data-testid="extension-encrypted-chooser"]') !== null`,
    ),
    false,
    'the chooser is shown',
  )
  ctx.step('an encrypted folder on the device stays closed to an extension')

  const extensions = await runAction(instance, 'wm.app.open', {
    appId: 'system.settings',
    at: '/extensions',
  })
  assert.ok(extensions.ok, JSON.stringify(extensions))
  await waitForLocation(instance, 'extensions')
  assert.equal(
    await instance.exec<boolean>(
      `return document.querySelector('[data-testid="extension-dev-mode"]') !== null`,
    ),
    false,
    'developer mode is offered',
  )
  ctx.step('developer mode is not offered')

  const agents = await runAction(instance, 'wm.app.open', {
    appId: 'system.settings',
    at: '/ai/providers',
  })
  assert.ok(agents.ok, JSON.stringify(agents))
  await waitForLocation(instance, 'ai.providers')
  await instance.waitForDisplayed('settings-not-on-this-device')
  ctx.step('the delegate settings say they are not available here')

  // The counter case of `mcp-agent` (spec 021 research R12): a phone has no `--mcp-agent` helper.
  assert.equal(table.mcpHelper, false)
  const mcp = await runAction(instance, 'wm.app.open', {
    appId: 'system.settings',
    at: '/ai/agents',
  })
  assert.ok(mcp.ok, JSON.stringify(mcp))
  await waitForLocation(instance, 'ai.agents')
  await instance.waitForDisplayed('settings-not-on-this-device')
  ctx.step('the agent access says it is not available here')

  // Spec 065 research R13: a phone has no agent browser; its settings say so, and the catalog it
  // pushes to Rust has no browser tool (`forDevice`, checked in check-agent-actions).
  assert.equal(table.agentBrowser, false)
  const browser = await runAction(instance, 'wm.app.open', {
    appId: 'system.settings',
    at: '/ai/browser',
  })
  assert.ok(browser.ok, JSON.stringify(browser))
  await waitForLocation(instance, 'ai.browser')
  await instance.waitForDisplayed('settings-not-on-this-device')
  ctx.step('the agent browser is not offered here')
})
