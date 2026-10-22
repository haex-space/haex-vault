// The settings actions of the External Bridge (spec 049, FR-001, FR-002, FR-013, FR-038,
// contracts/tauri-commands.md): turning it on, choosing its port, and changing, revoking and
// unblocking a browser extension. Guardrails: they open a port on this device or decide what an
// extension may read and write, which is the user's choice only, so no agent can call them. Part of
// `SETTINGS_ACTIONS`; the handlers are in `stores/settingsActionHandlers.ts`. Pure, relative `.ts`
// imports only.
import type { ActionDefinition } from './types.ts'

const OVERVIEW = {
  type: 'object',
  properties: {
    enabled: { type: 'boolean' },
    port: { type: 'number' },
    state: { type: 'string' },
    clients: { type: 'array' },
    blocked: { type: 'array' },
  },
  required: ['enabled', 'port', 'state', 'clients', 'blocked'],
} as const

const CLIENT = {
  type: 'object',
  properties: { clientId: { type: 'string' } },
  required: ['clientId'],
} as const

/** The fields every guardrail of the bridge shares. */
const GUARDRAIL = {
  result: OVERVIEW,
  target: 'none',
  scope: 'guardrails',
  agentCallable: false,
  binding: 'global',
  effect: 'write',
} as const

export const BROWSER_BRIDGE_ACTIONS: readonly ActionDefinition[] = [
  {
    id: 'settings.browserBridge.set',
    titleKey: 'actions.settings.browserBridge.set',
    description:
      'Turn the bridge for browser extensions on or off on this device and set its port (1024-65535). User only.',
    input: {
      type: 'object',
      properties: {
        enabled: { type: 'boolean' },
        port: { type: 'number' },
      },
      required: ['enabled', 'port'],
    },
    ...GUARDRAIL,
  },
  {
    id: 'settings.browserBridge.client.update',
    titleKey: 'actions.settings.browserBridge.client.update',
    description:
      'Change what an approved browser extension may do: kind, tag scope and default tag, never wider than it declared. User only.',
    input: {
      type: 'object',
      properties: {
        clientId: { type: 'string' },
        access: { type: 'string', enum: ['read', 'readWrite'] },
        scopeTags: {
          type: 'array',
          items: { type: 'string' },
          description: 'Tags of the scope; left out for every entry.',
        },
        defaultTag: {
          type: 'string',
          description: 'Tag of new entries; left out for none.',
        },
      },
      required: ['clientId', 'access'],
    },
    ...GUARDRAIL,
  },
  {
    id: 'settings.browserBridge.client.revoke',
    titleKey: 'actions.settings.browserBridge.client.revoke',
    description:
      'Withdraw the approval of a browser extension and close its connections. User only.',
    input: CLIENT,
    ...GUARDRAIL,
  },
  {
    id: 'settings.browserBridge.client.unblock',
    titleKey: 'actions.settings.browserBridge.client.unblock',
    description:
      'Lift the block of a browser extension; it is asked again on its next connect. User only.',
    input: CLIENT,
    ...GUARDRAIL,
  },
]
