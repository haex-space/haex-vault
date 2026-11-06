// Helpers for the scenarios of agent profiles (spec 083): the profiles are made and given to agents
// through the same catalog actions the pages run, so a scenario sets up what it needs without
// walking the pages each time. The pages themselves are walked in `agent-profiles.test.ts`.
import { runAction, type ActionOutcome } from './settings.ts'
import type { FlowInstance } from './flows.ts'

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

/** Puts the agent into the profile ("Standard" takes it out of every other one). */
export async function assignAgent(
  instance: FlowInstance,
  agentId: string,
  profileId: string,
): Promise<void> {
  ok(
    await runAction(instance, 'settings.agents.profiles.assign', {
      subject: `agent:${agentId}`,
      profileId,
    }),
    'settings.agents.profiles.assign',
  )
}

/** Puts a model of the local agent into the profile (spec 083 US5); `model` is its catalog id. */
export async function assignModel(
  instance: FlowInstance,
  model: string,
  profileId: string,
): Promise<void> {
  ok(
    await runAction(instance, 'settings.agents.profiles.assign', {
      subject: `model:${model}`,
      profileId,
    }),
    'settings.agents.profiles.assign',
  )
}

/** The id of the agent in the list "KI → Agenten", which has to be on screen. */
export async function agentIdOf(
  instance: FlowInstance,
  slug: string,
): Promise<string> {
  const row = `[data-testid="mcp-agent"][data-slug="${slug}"]`
  await instance.waitForDisplayed(row)
  const id = await instance.exec<string>(
    `return document.querySelector(arguments[0])?.getAttribute('data-agent-id') ?? ''`,
    [row],
  )
  if (!id) throw new Error(`no agent with the slug ${slug} in the list`)
  return id
}
