// The settings actions of the MCP access for external agents (spec 021, FR-001, FR-016,
// contracts/tauri-commands.md): turning it on or off on this device. Guardrails: they decide which
// agents may reach holzi at all, which is the user's choice only, so no agent can call them. Listed in
// `catalog.ts` (`settingsActions.ts` is at its size limit); the handlers are in `stores/settingsActionHandlers.ts`. Pure, relative `.ts`
// imports only.
import type { ActionDefinition } from './types.ts'

const STATUS = {
  type: 'object',
  properties: {
    enabled: { type: 'boolean' },
    state: { type: 'string' },
  },
  required: ['enabled', 'state'],
} as const

export const MCP_AGENT_ACTIONS: readonly ActionDefinition[] = [
  {
    id: 'settings.agents.mcp.enable',
    titleKey: 'actions.settings.agents.mcp.enable',
    description:
      'Turn the MCP access for external agents on or off on this device. User only.',
    input: {
      type: 'object',
      properties: { enabled: { type: 'boolean' } },
      required: ['enabled'],
    },
    result: STATUS,
    target: 'none',
    scope: 'guardrails',
    agentCallable: false,
    binding: 'global',
    effect: 'write',
  },
]
