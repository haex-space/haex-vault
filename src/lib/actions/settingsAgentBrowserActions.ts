// The settings actions of the agent browser (spec 065, data-model.md "Katalogaktionen"): the
// browser rights of the built-in agent, the browser chosen on this device, and removing the
// browser's profile. Guardrails: they decide what an agent may reach on the web, which is the
// user's choice only, so no agent can call them. The handlers are in
// `stores/settingsActionHandlers.ts`. Pure, relative `.ts` imports only.
import type { ActionDefinition } from './types.ts'

/** The fields every guardrail of the agent browser shares. */
const GUARDRAIL = {
  /** The commands answer nothing. */
  result: { type: 'object', nullable: true },
  target: 'none',
  scope: 'guardrails',
  agentCallable: false,
  binding: 'global',
} as const

export const AGENT_BROWSER_ACTIONS: readonly ActionDefinition[] = [
  {
    id: 'settings.agentBrowser.builtin.set',
    titleKey: 'actions.settings.agentBrowser.builtinSet',
    description:
      'Set the browser rights of the built-in agent on this device: the websites it may open, each to read or to operate; null switches the browser off for it.',
    input: {
      type: 'object',
      properties: {
        rights: {
          type: 'object',
          nullable: true,
          properties: {
            userBrowser: { type: 'boolean' },
            sites: { type: 'object' },
          },
        },
      },
      required: ['rights'],
    },
    effect: 'write',
    ...GUARDRAIL,
  },
  {
    id: 'settings.agentBrowser.path.set',
    titleKey: 'actions.settings.agentBrowser.pathSet',
    description:
      'Choose the browser program the agent browser starts on this device; null searches for one again.',
    input: {
      type: 'object',
      properties: { path: { type: 'string', nullable: true } },
      required: ['path'],
    },
    effect: 'write',
    ...GUARDRAIL,
  },
  {
    id: 'settings.agentBrowser.profile.clear',
    titleKey: 'actions.settings.agentBrowser.profileClear',
    description:
      'End the agent browser and remove its profile: every login of the agent browser on websites is gone.',
    input: { type: 'object' },
    effect: 'destructive',
    ...GUARDRAIL,
  },
]
