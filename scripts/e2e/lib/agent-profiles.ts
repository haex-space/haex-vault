// Helpers for the scenarios of agent profiles (spec 083): the profiles are made and given to agents
// through the same catalog actions the pages run, so a scenario sets up what it needs without
// walking the pages each time. The pages themselves are walked in `agent-profiles.test.ts`.
import { runAction, waitForLocation, type ActionOutcome } from './settings.ts'
import { unwrap, type FlowInstance } from './flows.ts'

/** The id of "Standard" (Rust `STANDARD_PROFILE_ID`). */
export const STANDARD_PROFILE_ID = '61705f70-726f-6669-6c65-5f7374640083'

function ok(outcome: ActionOutcome, what: string): ActionOutcome {
  if (!outcome.ok) throw new Error(`${what} failed: ${JSON.stringify(outcome)}`)
  return outcome
}

/** Creates an agent profile and, when `rights` is given, sets them; returns its id. */
export async function createProfile(
  instance: FlowInstance,
  name: string,
  rights?: Record<string, unknown>,
): Promise<string> {
  const created = ok(
    await runAction(instance, 'settings.agents.profiles.create', { name }),
    'settings.agents.profiles.create',
  )
  const id = (created.result as { id: string }).id
  if (rights) {
    ok(
      await runAction(instance, 'settings.agents.profiles.rights.set', {
        id,
        rights,
      }),
      'settings.agents.profiles.rights.set',
    )
  }
  return id
}

/** Adds the subject (`agent:<id>` or `model:<id>`) to the profile, or removes it from it. */
export async function setMember(
  instance: FlowInstance,
  subject: string,
  profileId: string,
  member = true,
): Promise<void> {
  ok(
    await runAction(instance, 'settings.agents.profiles.assign', {
      subject,
      profileId,
      member,
    }),
    'settings.agents.profiles.assign',
  )
}

/** Makes the profile the active one of the vault (no question: the action is what the page runs). */
export async function activateProfile(
  instance: FlowInstance,
  profileId: string,
): Promise<void> {
  ok(
    await runAction(instance, 'settings.agents.profiles.activate', {
      profileId,
    }),
    'settings.agents.profiles.activate',
  )
}

/**
 * Gives the agent the rights of the profile: it becomes a member and the profile becomes the active
 * one. "Standard" only needs to be active. Only one profile is active at a time, so a scenario that
 * needs two agents with different rights makes them members of one profile and leaves the other out.
 */
export async function assignAgent(
  instance: FlowInstance,
  agentId: string,
  profileId: string,
): Promise<void> {
  if (profileId !== STANDARD_PROFILE_ID) {
    await setMember(instance, `agent:${agentId}`, profileId)
  }
  await activateProfile(instance, profileId)
}

/** [`assignAgent`] for a model of the local agent (spec 083 US5); `model` is its catalog id. */
export async function assignModel(
  instance: FlowInstance,
  model: string,
  profileId: string,
): Promise<void> {
  if (profileId !== STANDARD_PROFILE_ID) {
    await setMember(instance, `model:${model}`, profileId)
  }
  await activateProfile(instance, profileId)
}

/** The id of the agent in the list "KI → Agenten und Modelle", which this opens. */
export async function agentIdOf(
  instance: FlowInstance,
  slug: string,
): Promise<string> {
  ok(
    await runAction(instance, 'wm.app.open', {
      appId: 'system.settings',
      at: '/ai/agents',
    }),
    'wm.app.open',
  )
  await waitForLocation(instance, 'ai.agents')
  const row = `[data-testid="mcp-agent"][data-slug="${slug}"]`
  await instance.waitForDisplayed(row)
  const id = await instance.exec<string>(
    `return document.querySelector(arguments[0])?.getAttribute('data-agent-id') ?? ''`,
    [row],
  )
  if (!id) throw new Error(`no agent with the slug ${slug} in the list`)
  return id
}

/** A rights group the scenarios of files and contacts set (`Rights` of the bindings). */
export type StandardRights = Record<string, unknown> & {
  files?: {
    device?: boolean
    storages?: Record<string, string>
    folders?: Record<string, { level: string; cloud?: boolean }>
    fileShares?: boolean
    syncRules?: boolean
  }
  contacts?: { all?: string; groups?: Record<string, string> }
}

/** The rights of "Standard", where the chat's agent and every agent in no active profile is judged. */
export async function standardRights(
  instance: FlowInstance,
): Promise<StandardRights> {
  const detail = unwrap<{ rights: StandardRights }>(
    'agent_profile_get',
    await instance.invoke('agent_profile_get', {
      args: { id: STANDARD_PROFILE_ID },
    }),
  )
  return detail.rights
}

/** Replaces groups of the rights of "Standard", as the user does in its profile; the others stay. */
export async function editStandardRights(
  instance: FlowInstance,
  patch: StandardRights,
): Promise<void> {
  const rights = await standardRights(instance)
  unwrap(
    'agent_profile_rights_set',
    await instance.invoke('agent_profile_rights_set', {
      args: { id: STANDARD_PROFILE_ID, rights: { ...rights, ...patch } },
    }),
  )
}

/** Opens the page of "Standard" in KI → Agentenprofile, where the groups of its rights are edited. */
export async function openStandardProfile(
  instance: FlowInstance,
): Promise<void> {
  ok(
    await runAction(instance, 'wm.app.open', {
      appId: 'system.settings',
      at: `/ai/profiles/${STANDARD_PROFILE_ID}`,
    }),
    'wm.app.open',
  )
  await waitForLocation(instance, 'ai.profiles.profile')
}
