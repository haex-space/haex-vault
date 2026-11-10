import assert from 'node:assert/strict'
import { join } from 'node:path'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { activateProfile, createProfile } from '../lib/agent-profiles.ts'
import { startMcpAgent, type ToolResult } from '../lib/mcp-agent.ts'
import { choose, openSettings, waitForLocation } from '../lib/settings.ts'
import {
  fixture,
  inFrame,
  openFromLauncher,
  type InstalledExtension,
} from '../lib/extensions.ts'

/** The count the test extension answered: holzi passes the extension's text on as `data`
 * (`{"extension": "Test-Erweiterung", "data": "{\"entries\":0}"}`). */
function entriesOf(result: ToolResult): number | undefined {
  const data = result.structuredContent?.data
  if (typeof data !== 'string') return undefined
  const parsed = JSON.parse(data) as { entries?: unknown }
  return typeof parsed.entries === 'number' ? parsed.entries : undefined
}

// Spec 018 (T042 path) and spec 021 US4: a tool of an extension is called in holzi's real window,
// through the extension's frame and its bundled vault-sdk. The harness runs no model, so an external
// agent makes the call over holzi's MCP server; holzi then opens its MCP connection to the frame
// exactly as for the built-in agent (`WindowLinks`). Called twice: once with the extension's tab
// closed (holzi opens a frame) and once with it open.
scenario('extension-agent-tools-call', { timeoutMs: 300_000 }, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-agent-tools-call' })

  const file = fixture('e2e', 'agent-tools')
  const preview = unwrap<{
    declared: Array<{ kind: string; action: string; target: string }>
  }>(
    'extension_install_preview',
    await instance.invoke('extension_install_preview', {
      source: { kind: 'file', file },
    }),
  )
  const accepted = preview.declared.map((row) => ({
    kind: row.kind,
    action: row.action,
    target: row.target,
    granted:
      row.kind === 'database' ||
      (row.kind === 'agentTool' && row.target === 'count_entries'),
  }))
  const extension = unwrap<InstalledExtension>(
    'extension_install',
    await instance.invoke('extension_install', {
      args: { source: { kind: 'file', file }, accepted, confirmed: false },
    }),
  )
  ctx.step('installed with count_entries for the agent')

  await openSettings(instance)
  await instance.click('settings-category-ai')
  await waitForLocation(instance, 'ai')
  await instance.click('settings-row-ai.access')
  await waitForLocation(instance, 'ai.access')
  await instance.click('mcp-switch')
  await instance.waitForDisplayed(
    '[data-testid="mcp-state"][data-state="running"]',
    30_000,
  )
  const toolsProfile = await createProfile(instance, 'Erweiterung', {
    operate: {},
    extensions: [extension.id],
  })
  const agent = startMcpAgent({
    app: ctx.app.path,
    slug: 'e2e-tools-agent',
    root: instance.root,
    marker: instance.marker,
    logFile: join(
      process.env.E2E_RUN_DIR ?? '.',
      'extension-agent-tools-call',
      'agent.log',
    ),
  })
  ctx.onTeardown(() => agent.stop())
  await agent.initialize()
  await instance.waitForDisplayed('mcp-registration')
  // Spec 083: the dialog picks an agent profile; this one makes the extension's tools visible.
  await choose(instance, '[data-testid="agent-profile-select"]', toolsProfile)
  await instance.click('mcp-registration-register')
  await agent.notified('notifications/tools/list_changed')
  // Only the active profile applies to its members (spec 083 FR-006a): the agent joined
  // "Erweiterung" in the dialog, and making that profile the active one gives it the tool.
  await activateProfile(instance, toolsProfile)
  const tool = 'x_agenttools_count_entries'
  const toolNames = async () =>
    (
      await agent.request<{ tools: Array<{ name: string }> }>('tools/list')
    ).tools.map((t) => t.name)
  await ctx.waitFor(
    `${tool} in the tools of the agent`,
    async () => (await toolNames()).includes(tool),
    { timeoutMs: 20_000 },
  )
  ctx.step('the agent sees the extension tool')

  const started = Date.now()
  const first = await agent.callTool(tool)
  assert.notEqual(first.isError, true, JSON.stringify(first))
  assert.equal(entriesOf(first), 0, JSON.stringify(first))
  ctx.step(`called with no open tab in ${Date.now() - started} ms`)

  await openFromLauncher(instance, extension)
  await ctx.waitFor(
    'the extension to count its entries in its tab',
    async () =>
      (await inFrame<string>(
        instance,
        extension,
        `return document.getElementById('count').textContent`,
      )) === '0',
    { timeoutMs: 20_000 },
  )
  const again = Date.now()
  const second = await agent.callTool(tool)
  assert.notEqual(second.isError, true, JSON.stringify(second))
  assert.equal(entriesOf(second), 0, JSON.stringify(second))
  ctx.step(`called with the tab open in ${Date.now() - again} ms`)
})
