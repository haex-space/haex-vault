// The settings actions of the MCP access for external agents (spec 021, FR-001, FR-016, FR-026 to
// FR-028, contracts/tauri-commands.md): turning it on or off on this device, and an agent's
// approval mode, revocation, block and log, and the registration of a chat delegate (US6). Rights are not set per agent any more: they belong to its agent profile (spec 083, `settingsAgentProfileActions.ts`). Guardrails: they decide which
// agents may reach holzi at all, which is the user's choice only, so no agent can call them. Listed in
// `catalog.ts` (`settingsActions.ts` is at its size limit); the handlers are in `stores/settingsActionHandlers.ts`. Pure, relative `.ts`
// imports only.
import type { ActionDefinition } from './types.ts'

const STATUS = {
  type: 'object',
  properties: {
    enabled: { type: 'boolean' },
    state: { type: 'string' },
    httpPort: { type: 'integer', nullable: true },
  },
  required: ['enabled', 'state'],
} as const

const AGENT_ID = {
  type: 'string',
  description: 'The id of a registered agent.',
} as const

const AGENT = {
  type: 'object',
  properties: { agentId: AGENT_ID },
  required: ['agentId'],
} as const

/** The fields every guardrail on one agent shares. */
const GUARDRAIL = {
  /** The commands answer nothing. */
  result: { type: 'object', nullable: true },
  target: 'none',
  scope: 'guardrails',
  agentCallable: false,
  binding: 'global',
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
  {
    id: 'settings.agents.mcp.revoke',
    titleKey: 'actions.settings.agents.mcp.revoke',
    description:
      'Revoke an external agent: end its connections, forget its agent profile assignment and its credential. User only.',
    input: AGENT,
    ...GUARDRAIL,
    effect: 'destructive',
  },
  {
    id: 'settings.agents.mcp.unblock',
    titleKey: 'actions.settings.agents.mcp.unblock',
    description:
      'Lift the block of an external agent; it may ask for a registration again. User only.',
    input: AGENT,
    ...GUARDRAIL,
    effect: 'write',
  },
  {
    id: 'settings.agents.mcp.log.clear',
    titleKey: 'actions.settings.agents.mcp.log.clear',
    description: 'Empty the log of an external agent. User only.',
    input: AGENT,
    ...GUARDRAIL,
    effect: 'destructive',
  },
]
