import assert from 'node:assert/strict'
import { join } from 'node:path'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import {
  STANDARD_PROFILE_ID,
  assignModel,
  createProfile,
  setMember,
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
  kind?: string
  missing?: boolean
  /** Whether the chat offers the model now (spec 083 FR-006b). */
  available?: boolean
  name: string
  /** The profile that applies now. */
  profileId: string
  profileName: string
  /** The profiles it is a member of ("Standard", which every subject is in, is not listed). */
  profiles: Array<{ id: string; name: string }>
}
type Detail = {
  rights: { browser?: { all?: string | null } }
  members: Subject[]
}

// Spec 083, quickstart M1 to M5: agent profiles. Two stand-in agents (spec 021) register and have
// the rights of "Standard", they see reading; making one a member of the profile "Vorsichtig" on its
// page and making "Vorsichtig" the active one takes its tools away and refuses its calls, while the
// other, which is no member, is unchanged (M1). Setting the browser of "Standard" applies at once
// with no question and gives every agent that is not in the active profile the browser tools, one
// that registers later too (M2). Adding an agent to another profile from that profile's page keeps
// it in the first, deleting a profile removes its memberships, and making "Standard" active applies
// at once (M3). A new agent is asked with the
// rights of "Standard" read only and a field to join a profile (M4). A model of the local agent is
// a member of a profile like an agent (M5). M9 (an expired
// agent is asked with its profile preselected) has no scenario: the rig has no way to age a
// registration, `agent_profiles` and `mcp_server` unit tests cover it.
scenario('agent-profiles', { timeoutMs: 360_000 }, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-agent-profiles' })

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

  // M1: the action the profile page runs makes it a member of "Vorsichtig", the list makes that profile active; its
  // tools go, the other agent, which is no member, keeps them.
  const careful = await createProfile(instance, 'Vorsichtig', { operate: {} })
  await setMember(instance, (await subjectOf('e2e-profile-a')).subject, careful)
  await ctx.waitFor('the agent to be a member', async () =>
    (await subjectOf('e2e-profile-a')).profiles.some(
      (profile) => profile.id === careful,
    ),
  )
  assert.ok(
    (await toolsOf(first)).includes('wm_state_get'),
    'a member of a profile that is not active has the rights of Standard',
  )
  await open('/ai/profiles', 'ai.profiles')
  await choose(instance, '[data-testid="agent-profile-active"]', careful)
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
  ctx.step('M1 stricter with a membership and the active profile')

  // M2: a change of "Standard" applies at once, with no question.
  await open('/ai/profiles', 'ai.profiles')
  await instance.click('[data-testid="agent-profile-row"][data-standard]')
  await waitForLocation(instance, 'ai.profiles.profile')
  await choose(instance, '[data-testid="mcp-browser-level"]', 'read')
  await ctx.waitFor('the agent in Standard to get the browser', async () =>
    (await toolsOf(second)).includes('browser_open'),
  )
  assert.equal(
    (await detailOf(STANDARD_PROFILE_ID)).rights.browser?.all,
    'read',
    'the change applied without a question',
  )
  const third = await connect('e2e-profile-c')
  assert.ok(
    (await toolsOf(third)).includes('browser_open'),
    'a new agent has it too',
  )
  ctx.step('M2 a change applies at once, a later agent has the rights')

  // M4: registering with a profile makes the agent a member; the profile is active, so it applies.
  const fourth = await connect('e2e-profile-d', careful)
  assert.equal((await toolsOf(fourth)).length, 0)
  const fourthSubject = await subjectOf('e2e-profile-d')
  assert.equal(fourthSubject.profileId, careful)
  assert.ok(fourthSubject.profiles.some((profile) => profile.id === careful))
  ctx.step('M4 registered as a member of a chosen profile')

  // M3: adding an agent from the page of another profile keeps it in the first.
  const other = await createProfile(instance, 'Dritte')
  const moving = await subjectOf('e2e-profile-a')
  assert.equal(moving.profileId, careful)
  await open(`/ai/profiles/${other}`, 'ai.profiles.profile')
  await choose(instance, '[data-testid="agent-profile-add"]', moving.subject)
  await ctx.waitFor('the agent to be added', async () =>
    (await detailOf(other)).members.some((m) => m.subject === moving.subject),
  )
  assert.ok(
    (await detailOf(careful)).members.some((m) => m.subject === moving.subject),
    'the profile it was in still lists it',
  )

  const two = await createProfile(instance, 'Zwei')
  await setMember(instance, (await subjectOf('e2e-profile-b')).subject, two)
  await setMember(instance, (await subjectOf('e2e-profile-c')).subject, two)
  await open(`/ai/profiles/${two}`, 'ai.profiles.profile')
  await instance.click('agent-profile-delete')
  const text = await instance.exec<string>(
    `return document.querySelector('[data-testid="agent-profile-delete-text"]')?.textContent ?? ''`,
  )
  assert.match(
    text,
    /2/,
    'the page names the number of agents that are members',
  )
  await instance.click('agent-profile-delete-confirm')
  await waitForLocation(instance, 'ai.profiles')
  assert.equal((await subjectOf('e2e-profile-b')).profiles.length, 0)
  assert.equal((await subjectOf('e2e-profile-c')).profiles.length, 0)

  // Making "Standard" active gives the members of "Vorsichtig" more, with no question.
  await choose(
    instance,
    '[data-testid="agent-profile-active"]',
    STANDARD_PROFILE_ID,
  )
  await ctx.waitFor('the member to have the rights of Standard', async () =>
    (await toolsOf(first)).includes('wm_state_get'),
  )
  ctx.step(
    'M3 several profiles, delete removes memberships, activation applies',
  )

  // M5: a model of the local agent is in a profile like an agent. The vault of the rig offers no
  // model, so one that is a member and not offered stands in: it is listed under "Lokaler Agent"
  // with its profile and marked as gone, the profile's page lists it, and it leaves the profile.
  const modelProfile = await createProfile(instance, 'Modelle')
  await assignModel(instance, 'e2e-model', modelProfile)
  const model = (await subjects()).find(
    (subject) => subject.subject === 'model:e2e-model',
  )
  assert.ok(model, 'the assigned model is listed')
  assert.equal(model.kind, 'builtin')
  assert.equal(model.profileId, modelProfile)
  assert.equal(
    model.available,
    true,
    'a member of the active profile is offered',
  )
  assert.equal(
    model.missing,
    true,
    'a model the vault does not offer is marked',
  )
  await open(`/ai/profiles/${modelProfile}`, 'ai.profiles.profile')
  const modelRow =
    '[data-testid="agent-profile-member"][data-subject="model:e2e-model"]'
  await instance.waitForDisplayed(modelRow)
  await instance.click(
    `${modelRow} [data-testid="agent-profile-member-remove"]`,
  )
  await ctx.waitFor('the model to leave the profile', async () =>
    (await subjects()).every(
      (subject) =>
        subject.subject !== 'model:e2e-model' || subject.profiles.length === 0,
    ),
  )
  const left = (await subjects()).find(
    (subject) => subject.subject === 'model:e2e-model',
  )
  assert.equal(
    left?.available ?? false,
    false,
    'a model that is not in the active profile is not offered',
  )
  ctx.step('M5 a model of the local agent in a profile')

  for (const agent of agents) await agent.stop()
})
