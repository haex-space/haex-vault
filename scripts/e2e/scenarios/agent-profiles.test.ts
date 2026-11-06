import assert from 'node:assert/strict'
import { join } from 'node:path'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import {
  STANDARD_PROFILE_ID,
  assignAgent,
  createProfile,
} from '../lib/agent-profiles.ts'
import { startMcpAgent, type McpAgent } from '../lib/mcp-agent.ts'
import {
  choose,
  openSettings,
  runAction,
  waitForLocation,
} from '../lib/settings.ts'

type Subject = {
  subject: string
  name: string
  profileId: string
  profileName: string
}
type Detail = {
  rights: { browser?: { all?: string | null } }
  members: Subject[]
}

// Spec 083, quickstart M1 to M4: agent profiles. Two stand-in agents (spec 021) register in "Standard"
// and see reading; putting one into the profile "Vorsichtig" on its page takes its tools away and
// refuses its calls, while the other is unchanged (M1). Setting the browser of "Standard" asks first,
// naming the number of agents; cancelling leaves it, confirming gives every agent in "Standard" the
// browser tools, one that registers later too (M2). Adding an agent to another profile from that
// profile's page moves it, and deleting a profile moves its agents to "Standard" (M3). A new agent
// is asked with the rights of "Standard" read only and a field "Agentenprofil" (M4). M9 (an expired
// agent is asked with its profile preselected) has no scenario: the rig has no way to age a
// registration, `agent_profiles` and `mcp_server` unit tests cover it.
scenario('agent-profiles', { timeoutMs: 360_000 }, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-agent-profiles' })

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

  const agents: McpAgent[] = []
  /** Connects a stand-in agent and registers it in Auto, in `profileId` when the dialog is to choose. */
  async function connect(slug: string, profileId?: string): Promise<McpAgent> {
    const agent = startMcpAgent({
      app: ctx.app.path,
      slug,
      root: instance.root,
      marker: instance.marker,
      logFile: join(
        process.env.E2E_RUN_DIR ?? '.',
        'agent-profiles',
        `${slug}.log`,
      ),
    })
    agents.push(agent)
    ctx.onTeardown(() => agent.stop())
    await agent.initialize()
    await instance.waitForDisplayed('mcp-registration')
    await instance.waitForDisplayed('agent-profile-select')
    if (profileId) {
      await choose(instance, '[data-testid="agent-profile-select"]', profileId)
    }
    await instance.click('mcp-mode-auto')
    await instance.click('mcp-registration-register')
    await agent.notified('notifications/tools/list_changed')
    return agent
  }
  const toolsOf = async (agent: McpAgent) =>
    (
      await agent.request<{ tools: Array<{ name: string }> }>('tools/list')
    ).tools.map((tool) => tool.name)
  const open = async (at: string, location: string) => {
    const opened = await runAction(instance, 'wm.app.open', {
      appId: 'system.settings',
      at,
    })
    assert.ok(opened.ok, JSON.stringify(opened))
    await waitForLocation(instance, location)
  }
  const subjects = async () =>
    unwrap<Subject[]>(
      'agent_subjects_list',
      await instance.invoke('agent_subjects_list'),
    )
  const subjectOf = async (slug: string) => {
    const found = (await subjects()).find((subject) =>
      subject.name.includes(slug),
    )
    assert.ok(found, `no agent named like ${slug}`)
    return found
  }
  const detailOf = async (id: string) =>
    unwrap<Detail>(
      'agent_profile_get',
      await instance.invoke('agent_profile_get', { args: { id } }),
    )

  // M4: a new agent is asked with the rights of "Standard", read only, and a field for the profile.
  const first = await connect('e2e-profile-a')
  const second = await connect('e2e-profile-b')
  ctx.step('two agents registered in Standard')
  assert.ok((await toolsOf(first)).includes('wm_state_get'), 'Standard reads')
  assert.ok((await toolsOf(second)).includes('wm_state_get'))
  const standardOf = async (slug: string) =>
    (await subjectOf(slug)).profileId === STANDARD_PROFILE_ID
  assert.ok(await standardOf('e2e-profile-a'))
  assert.ok(await standardOf('e2e-profile-b'))

  // M1: the agent's page picks "Vorsichtig"; its tools go, the other agent keeps them.
  const careful = await createProfile(instance, 'Vorsichtig', { operate: {} })
  await open('/ai/agents', 'ai.agents')
  await instance.click(`[data-testid="mcp-agent"][data-slug="e2e-profile-a"]`)
  await waitForLocation(instance, 'ai.agents.agent')
  await choose(instance, '[data-testid="agent-profile-select"]', careful)
  await ctx.waitFor(
    'the agent to lose its tools',
    async () => (await toolsOf(first)).length === 0,
  )
  const refused = await first.callTool('wm_state_get')
  assert.equal(refused.isError, true)
  assert.equal(refused.structuredContent?.code, 'not_allowed')
  assert.ok(
    (await toolsOf(second)).includes('wm_state_get'),
    'the other is unchanged',
  )
  ctx.step('M1 stricter in one step')

  // M2: "Standard" asks before it gives more, naming the number of agents, also for the first right.
  await open('/ai/profiles', 'ai.profiles')
  await instance.click('[data-testid="agent-profile-row"][data-standard]')
  await waitForLocation(instance, 'ai.profiles.profile')
  await choose(instance, '[data-testid="mcp-browser-level"]', 'read')
  await instance.waitForDisplayed('agent-profile-widening')
  const counted = await instance.exec<string | null>(
    `return document.querySelector('[data-testid="agent-profile-widening"]')?.getAttribute('data-count') ?? null`,
  )
  assert.equal(counted, '1', 'one agent is in Standard')
  await instance.click('agent-profile-widening-confirm')
  await ctx.waitFor('the agent in Standard to get the browser', async () =>
    (await toolsOf(second)).includes('browser_open'),
  )
  ctx.step('M2 a widening change of Standard asked and applied')

  await choose(instance, '[data-testid="mcp-browser-level"]', 'write')
  await instance.waitForDisplayed('agent-profile-widening')
  await instance.click('agent-profile-widening-cancel')
  await ctx.waitFor('the question to close', async () =>
    instance.exec<boolean>(
      `return document.querySelector('[data-testid="agent-profile-widening"]') === null`,
    ),
  )
  assert.equal(
    (await detailOf(STANDARD_PROFILE_ID)).rights.browser?.all,
    'read',
    'cancelling leaves the profile as it was',
  )
  const third = await connect('e2e-profile-c')
  assert.ok(
    (await toolsOf(third)).includes('browser_open'),
    'a new agent has it too',
  )
  ctx.step('M2 cancelling restores, a later agent has the rights')

  // M4: registering with a profile puts the agent there.
  const fourth = await connect('e2e-profile-d', careful)
  assert.equal((await toolsOf(fourth)).length, 0)
  assert.equal((await subjectOf('e2e-profile-d')).profileId, careful)
  ctx.step('M4 registered into a chosen profile')

  // M3: adding an agent from the page of another profile moves it.
  const other = await createProfile(instance, 'Dritte')
  const moving = await subjectOf('e2e-profile-a')
  assert.equal(moving.profileId, careful)
  await open(`/ai/profiles/${other}`, 'ai.profiles.profile')
  await choose(instance, '[data-testid="agent-profile-add"]', moving.subject)
  await ctx.waitFor('the agent to move', async () =>
    (await detailOf(other)).members.some((m) => m.subject === moving.subject),
  )
  assert.ok(
    !(await detailOf(careful)).members.some(
      (m) => m.subject === moving.subject,
    ),
    'the profile it left no longer lists it',
  )

  const two = await createProfile(instance, 'Zwei')
  await assignAgent(
    instance,
    (await subjectOf('e2e-profile-b')).subject.slice('agent:'.length),
    two,
  )
  await assignAgent(
    instance,
    (await subjectOf('e2e-profile-c')).subject.slice('agent:'.length),
    two,
  )
  await open(`/ai/profiles/${two}`, 'ai.profiles.profile')
  await instance.click('agent-profile-delete')
  const text = await instance.exec<string>(
    `return document.querySelector('[data-testid="agent-profile-delete-text"]')?.textContent ?? ''`,
  )
  assert.match(text, /2/, 'the page names the number of agents that move')
  await instance.click('agent-profile-delete-confirm')
  await waitForLocation(instance, 'ai.profiles')
  assert.ok(await standardOf('e2e-profile-b'))
  assert.ok(await standardOf('e2e-profile-c'))
  ctx.step('M3 one profile at most, delete moves to Standard')

  for (const agent of agents) await agent.stop()
})
