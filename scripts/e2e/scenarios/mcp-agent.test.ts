import assert from 'node:assert/strict'
import { join } from 'node:path'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import { buildInstanceEnv } from '../lib/instance.ts'
import { startMcpAgent } from '../lib/mcp-agent.ts'
import { choose, openSettings, waitForLocation } from '../lib/settings.ts'

// Spec 021, US1 and US3 (T055, quickstart M1–M4 without Claude Code): the access for external agents
// is off until it is turned on in Einstellungen → KI → Agenten. A stand-in agent starts holzi's own
// `--mcp-agent` helper, holzi asks with the name the agent reports, and the user grants changing in
// "Fenster und Arbeitsbereiche" with the approval mode Auto. Then the agent reads the window state
// and creates a workspace without a prompt, and asks to delete it: holzi shows the call prompt as
// destructive, the user declines, the agent hears `user_declined` and the workspace stays.
scenario('mcp-agent', { timeoutMs: 240_000 }, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-mcp-agent' })

  await openSettings(instance)
  await instance.click('settings-category-ai')
  await waitForLocation(instance, 'ai')
  await instance.click('settings-row-ai.agents')
  await waitForLocation(instance, 'ai.agents')
  await instance.waitForDisplayed('[data-testid="mcp-state"][data-state="off"]')
  ctx.step('off by default')

  await instance.click('mcp-switch')
  await instance.waitForDisplayed(
    '[data-testid="mcp-state"][data-state="running"]',
  )
  ctx.step('turned on')

  const agent = startMcpAgent({
    app: ctx.app.path,
    slug: 'e2e-agent',
    env: buildInstanceEnv(instance.root, instance.marker),
    logFile: join(process.env.E2E_RUN_DIR ?? '.', 'mcp-agent', 'agent.log'),
  })
  ctx.onTeardown(() => agent.stop())
  await agent.initialize()
  await instance.waitForDisplayed('mcp-registration')
  const name = await instance.exec<string>(
    `return document.querySelector('[data-testid="mcp-registration-name"]')?.textContent?.trim() ?? ''`,
  )
  assert.match(name, /e2e-agent/)
  ctx.step('asked with the reported name')

  await choose(instance, '[data-testid="mcp-right-wm.layout"]', 'write')
  await instance.click('mcp-mode-auto')
  await instance.click('mcp-registration-register')
  await agent.notified('notifications/tools/list_changed')
  const listed = await agent.request<{ tools: Array<{ name: string }> }>(
    'tools/list',
  )
  const names = listed.tools.map((tool) => tool.name)
  for (const tool of [
    'wm_state_get',
    'wm_workspace_create',
    'wm_workspace_delete',
  ])
    assert.ok(names.includes(tool), `${tool} in ${names.join(', ')}`)
  assert.ok(
    !names.some((tool) => tool.startsWith('settings_agents_mcp')),
    'no guardrail',
  )
  ctx.step('registered with changing in the window manager, Auto')

  const read = await agent.callTool('wm_state_get')
  assert.notEqual(read.isError, true, JSON.stringify(read))
  const created = await agent.callTool('wm_workspace_create')
  assert.notEqual(created.isError, true, JSON.stringify(created))
  const workspaceId = created.structuredContent?.workspaceId
  assert.equal(typeof workspaceId, 'string', JSON.stringify(created))
  const prompts = await instance.exec<number>(
    `return document.querySelectorAll('[data-testid="mcp-call"]').length`,
  )
  assert.equal(prompts, 0, 'Auto runs reading and changing without a prompt')
  ctx.step('reading and changing ran without a prompt')

  const deleting = agent.callTool('wm_workspace_delete', { workspaceId })
  await instance.waitForDisplayed(
    '[data-testid="mcp-call-effect"][data-effect="destructive"]',
  )
  await instance.click('mcp-call-deny')
  const declined = await deleting
  assert.equal(declined.isError, true)
  assert.equal(declined.structuredContent?.code, 'user_declined')
  const after = await agent.callTool('wm_state_get')
  assert.ok(
    JSON.stringify(after.structuredContent).includes(String(workspaceId)),
    'the workspace stays',
  )
  ctx.step('the destructive call asked, was declined and did not run')
})
