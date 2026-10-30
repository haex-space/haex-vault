import assert from 'node:assert/strict'
import { createServer, type Server } from 'node:http'
import type { AddressInfo } from 'node:net'
import { join } from 'node:path'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import { startMcpAgent, type ToolResult } from '../lib/mcp-agent.ts'
import { openSettings, runAction, waitForLocation } from '../lib/settings.ts'

/** A website on 127.0.0.1: a page to read, and `/hang`, which never answers. */
function website(): Promise<{
  server: Server
  origin: string
  hangStarted: Promise<void>
}> {
  let onHangStarted!: () => void
  const hangStarted = new Promise<void>((resolve) => {
    onHangStarted = resolve
  })
  const server = createServer((request, response) => {
    if (request.url === '/hang') {
      onHangStarted()
      return
    }
    response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' })
    response.end(
      '<!doctype html><title>E2E-Seite</title><main><h1>Hallo vom Testserver</h1></main>',
    )
  })
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => {
      const port = (server.address() as AddressInfo).port
      resolve({ server, origin: `http://127.0.0.1:${port}`, hangStarted })
    })
  })
}

function content(result: ToolResult): Record<string, unknown> {
  return (result.structuredContent?.untrusted_page_content ?? {}) as Record<
    string,
    unknown
  >
}

// Spec 065, US1 and US3 (T060, quickstart M1 and M5 in part): the stand-in agent of spec 021,
// registered in Auto with the rights group "browser" for one website, opens and reads a page in
// holzi's own Chromium. The desktop shows its session in the sessions bar with the page's title;
// "Stoppen" there ends a call that waits for a page that never answers, and the agent's further
// browser calls are refused until it connects anew.
scenario('agent-browser', { timeoutMs: 300_000 }, async (ctx) => {
  const { server, origin, hangStarted } = await website()
  ctx.onTeardown(
    () => new Promise<void>((resolve) => server.close(() => resolve())),
  )
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-agent-browser' })

  await openSettings(instance)
  await instance.click('settings-category-ai')
  await waitForLocation(instance, 'ai')
  await instance.click('settings-row-ai.agents')
  await waitForLocation(instance, 'ai.agents')
  await instance.click('mcp-switch')
  await instance.waitForDisplayed(
    '[data-testid="mcp-state"][data-state="running"]',
    30_000,
  )

  const agent = startMcpAgent({
    app: ctx.app.path,
    slug: 'e2e-browser-agent',
    root: instance.root,
    marker: instance.marker,
    logFile: join(process.env.E2E_RUN_DIR ?? '.', 'agent-browser', 'agent.log'),
  })
  ctx.onTeardown(() => agent.stop())
  await agent.initialize()
  await instance.waitForDisplayed('mcp-registration')
  await instance.click('mcp-mode-auto')
  await instance.click('mcp-registration-register')
  await agent.notified('notifications/tools/list_changed')
  ctx.step('registered in Auto')

  const row = '[data-testid="mcp-agent"][data-slug="e2e-browser-agent"]'
  await instance.waitForDisplayed(row)
  const agentId = await instance.exec<string>(
    `return document.querySelector(arguments[0])?.getAttribute('data-agent-id') ?? ''`,
    [row],
  )
  assert.ok(agentId, 'the agent has an id')
  const granted = await runAction(instance, 'settings.agents.mcp.grants.set', {
    agentId,
    rights: {
      operate: {},
      browser: {
        userBrowser: false,
        sites: { [origin]: { level: 'read', fillRemembered: false } },
      },
    },
  })
  assert.ok(granted.ok, JSON.stringify(granted))
  await ctx.waitFor('the browser tools', async () => {
    const listed = await agent.request<{ tools: Array<{ name: string }> }>(
      'tools/list',
    )
    return listed.tools.some((tool) => tool.name === 'browser_open')
  })
  ctx.step('the rights group "browser" for one website')

  const opened = await agent.callTool('browser_open', { url: `${origin}/` })
  assert.notEqual(opened.isError, true, JSON.stringify(opened))
  assert.equal(content(opened).title, 'E2E-Seite', JSON.stringify(opened))
  assert.match(String(content(opened).text), /Hallo vom Testserver/)
  ctx.step('opened and read a page in the agent browser')

  await instance.waitForDisplayed('agent-browser-session', 15_000)
  const tabs = await instance.exec<string>(
    `return document.querySelector('[data-testid="agent-browser-session-tabs"]')?.textContent ?? ''`,
  )
  assert.match(tabs, /E2E-Seite/)
  ctx.step('the sessions bar shows the agent and its tab')

  const hanging = agent.callTool('browser_open', { url: `${origin}/hang` })
  await hangStarted
  await instance.waitForDisplayed('agent-browser-session-stop')
  const clicked = performance.now()
  await instance.click('agent-browser-session-stop')
  const stopped = await hanging
  assert.ok(
    performance.now() - clicked < 5_000,
    'the waiting call ended at once',
  )
  assert.equal(stopped.isError, true, JSON.stringify(stopped))
  assert.equal(stopped.structuredContent?.code, 'stopped')
  await ctx.waitFor(
    'the sessions bar to go',
    async () =>
      (await instance.exec<number>(
        `return document.querySelectorAll('[data-testid="agent-browser-session"]').length`,
      )) === 0,
  )
  const again = await agent.callTool('browser_tabs_list')
  assert.equal(again.structuredContent?.code, 'stopped', JSON.stringify(again))
  ctx.step('stopped from the sessions bar; further calls are refused')
})
