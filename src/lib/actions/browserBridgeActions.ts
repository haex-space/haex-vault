// The settings action of the External Bridge (spec 049, FR-001, FR-002, FR-013,
// contracts/tauri-commands.md): turning it on and choosing its port. A guardrail: it opens a port on
// this device, which is the user's choice only, so no agent can call it. Part of `SETTINGS_ACTIONS`;
// the handler is in `stores/settingsActionHandlers.ts`. Pure, relative `.ts` imports only.
import type { ActionDefinition } from './types.ts'

const OVERVIEW = {
  type: 'object',
  properties: {
    enabled: { type: 'boolean' },
    port: { type: 'number' },
    state: { type: 'string' },
  },
  required: ['enabled', 'port', 'state'],
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
    result: OVERVIEW,
    target: 'none',
    scope: 'guardrails',
    agentCallable: false,
    binding: 'global',
    effect: 'write',
  },
]
