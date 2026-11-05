// The settings actions of the agent browser (spec 065, data-model.md "Katalogaktionen"): the
// browser and password rights of the built-in agent, the browser chosen on this device, and
// removing the browser's profile. Guardrails: they decide what an agent may reach on the web, which is the
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
      'Set the browser rights of the built-in agent on this device: the level for all websites, and the listed websites that are exceptions, each to read, to operate or blocked; null switches the browser off for it.',
    input: {
      type: 'object',
      properties: {
        rights: {
          type: 'object',
          nullable: true,
          properties: {
            userBrowser: { type: 'boolean' },
            sites: { type: 'object' },
            /** The rules for the requests a page sends to a website (spec 065 FR-044). */
            requests: { type: 'object' },
            all: {
              type: 'string',
              enum: ['read', 'operate'],
              nullable: true,
              description:
                'The level for every website that is not listed; null is none.',
            },
          },
        },
      },
      required: ['rights'],
    },
    effect: 'write',
    ...GUARDRAIL,
  },
  {
    id: 'settings.agentBrowser.builtinPasswords.set',
    titleKey: 'actions.settings.agentBrowser.builtinPasswordsSet',
    description:
      'Set which entries of the password manager the built-in agent may have filled into websites of the agent browser: all of them or those with chosen tags, to read or also to save new logins (with the tag new entries get); null switches it off. The agent never sees the values.',
    input: {
      type: 'object',
      properties: {
        rights: {
          type: 'object',
          nullable: true,
          properties: {
            level: { type: 'string', enum: ['read', 'readWrite'] },
            tags: { type: 'array', items: { type: 'string' }, nullable: true },
            defaultTag: { type: 'string', nullable: true },
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
    id: 'settings.agentBrowser.trackers.set',
    titleKey: 'actions.settings.agentBrowser.trackersSet',
    description:
      'Set the tracker blocking of the agent browser on this device: blocking as a whole, the default filter lists switched off, and own lists by their https address. Answers the state of the lists.',
    input: {
      type: 'object',
      properties: {
        settings: {
          type: 'object',
          properties: {
            enabled: { type: 'boolean' },
            off: { type: 'array', items: { type: 'string' } },
            custom: {
              type: 'array',
              items: {
                type: 'object',
                properties: {
                  id: { type: 'string' },
                  name: { type: 'string' },
                  url: { type: 'string' },
                  enabled: { type: 'boolean' },
                },
                required: ['url'],
              },
            },
          },
          required: ['enabled'],
        },
      },
      required: ['settings'],
    },
    effect: 'write',
    ...GUARDRAIL,
    result: { type: 'object' },
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
