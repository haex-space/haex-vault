import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { connectOverHttp } from '../lib/mcp-http.ts'
import { runAction, waitForLocation } from '../lib/settings.ts'

// Spec 021 T097 (US1, US8 on Android, quickstart M9): a phone has no `--mcp-agent` helper, so
// external agents reach holzi over loopback HTTP with OAuth. The access is off until it is turned
// on in Einstellungen → KI → Externe Agenten; then the settings show the HTTP snippets with holzi's
// port and none for the program way. An MCP client in the test driver (through `adb forward`)
// registers, opens the authorization page, the person registers it in holzi's dialog with the
// preset rights (reading), and the client lists and calls a reading tool.
scenario(
  'android-mcp-agent',
  { needs: { phone: true }, timeoutMs: 240_000 },
  async (ctx) => {
    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'phone-mcp' })
    const table = unwrap<Record<string, unknown>>(
      'platform_capabilities',
      await instance.invoke('platform_capabilities'),
    )
    assert.equal(table.mcpHelper, false)
    assert.equal(table.mcpHttp, true)

    const opened = await runAction(instance, 'wm.app.open', {
      appId: 'system.settings',
      at: '/ai/agents',
    })
    assert.ok(opened.ok, JSON.stringify(opened))
    await waitForLocation(instance, 'ai.agents')
    await instance.waitForDisplayed(
      '[data-testid="mcp-state"][data-state="off"]',
    )
    await instance.click('mcp-switch')
    await instance.waitForDisplayed(
      '[data-testid="mcp-state"][data-state="running"]',
      30_000,
    )
    ctx.step('turned on')

    const status = unwrap<{ httpPort: number | null }>(
      'mcp_status',
      await instance.invoke('mcp_status'),
    )
    const port = status.httpPort
    assert.ok(port !== null && port > 0, JSON.stringify(status))
    await instance.waitForDisplayed('mcp-copy-http-claudeCode')
    assert.equal(
      await instance.exec<boolean>(
        `return document.querySelector('[data-testid="mcp-copy-claudeCode"]') !== null`,
      ),
      false,
      'a snippet for the program way',
    )
    ctx.step('the HTTP snippets name the port')

    const phone = instance.phone
    assert.ok(phone !== undefined, 'a phone has its controls')
    ctx.onTeardown(phone.reachAppPort(port))
    const client = await connectOverHttp(
      `http://127.0.0.1:${port}`,
      'E2E Phone Agent',
      async () => {
        await instance.waitForDisplayed('mcp-registration')
        await instance.click('mcp-registration-register')
      },
    )
    ctx.step('registered through the dialog')

    const listed = await client.request<{ tools: Array<{ name: string }> }>(
      'tools/list',
    )
    const names = listed.tools.map((tool) => tool.name)
    assert.ok(names.includes('wm_state_get'), names.join(', '))
    assert.ok(
      !names.some((name) => name.startsWith('settings_agents_mcp')),
      'no guardrail',
    )
    const read = await client.request<{ isError?: boolean }>('tools/call', {
      name: 'wm_state_get',
      arguments: {},
    })
    assert.notEqual(read.isError, true, JSON.stringify(read))
    await instance.waitForDisplayed('mcp-agents')
    ctx.step('the agent reads the window state and is listed')
  },
)
