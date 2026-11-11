import assert from 'node:assert/strict'
import { join } from 'node:path'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import { activateProfile, createProfile } from '../lib/agent-profiles.ts'
import { startMcpAgent } from '../lib/mcp-agent.ts'
import {
  choose,
  openSettings,
  runAction,
  waitForLocation,
} from '../lib/settings.ts'

// Spec 021, US1, US3 and US7 (T055, T063, T064, quickstart M1–M4 and M8 without Claude Code): the access for external agents
// is off until it is turned on in Einstellungen → KI → Agenten. A stand-in agent starts holzi's own
// `--mcp-agent` helper, holzi asks with the name the agent reports, and the user registers it in an
// agent profile that gives changing in "Fenster und Arbeitsbereiche" (spec 083: the rights come from
// the profile, not from the dialog), with the approval mode Auto. Then the agent reads the window state
// and creates a workspace without a prompt, and asks to delete it: holzi shows the call prompt as
// destructive, the user declines, the agent hears `user_declined` and the workspace stays. The
// settings list the agent as connected, its log shows the declined call, and revoking it ends the
// agent's connection at once.
scenario('mcp-agent', { timeoutMs: 240_000 }, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-mcp-agent' })

  await openSettings(instance)
  await instance.click('settings-category-ai')
  await waitForLocation(instance, 'ai')
  await instance.click('settings-row-ai.access')
  await waitForLocation(instance, 'ai.access')
  await instance.waitForDisplayed('[data-testid="mcp-state"][data-state="off"]')
  ctx.step('off by default')

  await instance.click('mcp-switch')
  await instance.waitForDisplayed(
    '[data-testid="mcp-state"][data-state="running"]',
    30_000,
  )
  ctx.step('turned on')

  const profileId = await createProfile(instance, 'Fenster ändern', {
    operate: {
      'wm.read': 'write',
      'wm.navigation': 'write',
      'wm.layout': 'write',
    },
  })
  ctx.step('an agent profile that changes windows')

  const agent = startMcpAgent({
    app: ctx.app.path,
    slug: 'e2e-agent',
    root: instance.root,
    marker: instance.marker,
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

  // The profile gives its members its rights while it is the active one.
  await activateProfile(instance, profileId)
  await choose(instance, '[data-testid="agent-profile-select"]', profileId)
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
  ctx.step(
    'registered in the agent profile with changing in the window manager, Auto',
  )

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

  // US7 (T063, T064): the agent in the list, its log and its revocation. Creating the workspace
  // moved the view away from the settings, so they open again at the external access.
  const opened = await runAction(instance, 'wm.app.open', {
    appId: 'system.settings',
    at: '/ai/access',
  })
  assert.ok(opened.ok, JSON.stringify(opened))
  await waitForLocation(instance, 'ai.access')
  const row = '[data-testid="mcp-agent"][data-slug="e2e-agent"]'
  await instance.waitForDisplayed(`${row}[data-connected]`)
  ctx.step('listed as connected')

  await instance.click(`${row} [data-testid="mcp-agent-log"]`)
  await waitForLocation(instance, 'ai.access.log')
  await instance.waitForDisplayed(
    '[data-testid="mcp-log-entry"][data-outcome="denied_user"]',
  )
  const logged = await instance.exec<number>(
    `return document.querySelectorAll('[data-testid="mcp-log-entry"]').length`,
  )
  assert.ok(logged >= 4, `${logged} entries`)
  ctx.step('the log lists the calls, the declined one included')

  const back = await runAction(instance, 'wm.app.open', {
    appId: 'system.settings',
    at: '/ai/access',
  })
  assert.ok(back.ok, JSON.stringify(back))
  await waitForLocation(instance, 'ai.access')
  await instance.click(`${row} [data-testid="mcp-agent-revoke"]`)
  await instance.click(`${row} [data-testid="mcp-agent-revoke-confirm"]`)
  await ctx.waitFor('the agent to be cut off', () => agent.exited())
  ctx.step('revoked: the connection ends at once')
})
