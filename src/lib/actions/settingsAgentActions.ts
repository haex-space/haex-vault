// The settings actions for what agents may reach (spec 044 FR-031b, spec 048 FR-033, spec 051
// FR-030, spec 056 FR-045, spec 061 FR-035): the stored permissions of agents for the files of this device, for
// storages, encrypted folders, the contacts and managing file shares and sync rules. Guardrails: only the user changes them. Part of `SETTINGS_ACTIONS`;
// handlers in `stores/settingsActionHandlers.ts`. Pure, relative `.ts` imports only.
import type { ActionDefinition, JsonSchema } from './types.ts'

const DONE: JsonSchema = {
  type: 'object',
  properties: { done: { type: 'boolean' } },
}

export const SETTINGS_AGENT_ACTIONS: readonly ActionDefinition[] = [
  {
    id: 'settings.agents.files.set',
    titleKey: 'actions.settings.agents.files.set',
    description:
      "Set an agent's permission for the files of this device, for one storage (another own device or a received share counts as one), for the contacts, or for managing file shares or sync rules. User only.",
    input: {
      type: 'object',
      properties: {
        agentId: { type: 'string' },
        kind: {
          type: 'string',
          enum: ['device', 'storage', 'contacts', 'fileShares', 'syncRules'],
        },
        target: {
          type: 'string',
          description:
            "Empty for the device, file shares and sync rules, the storage id for a storage, '*' or 'group:<id>' for the contacts.",
        },
        status: {
          type: 'string',
          enum: ['granted', 'read', 'readWrite', 'denied'],
          description:
            "'granted' or 'denied' for the device, file shares and sync rules; 'read', 'readWrite' or 'denied' for a storage or the contacts.",
        },
      },
      required: ['agentId', 'kind', 'target', 'status'],
    },
    result: DONE,
    target: 'none',
    scope: 'guardrails',
    effect: 'write',
    agentCallable: false,
    binding: 'global',
  },
  {
    id: 'settings.agents.files.remove',
    titleKey: 'actions.settings.agents.files.remove',
    description:
      "Remove an agent's remembered file permission, such as one for an encrypted folder; the agent is asked again. User only.",
    input: {
      type: 'object',
      properties: {
        agentId: { type: 'string' },
        kind: {
          type: 'string',
          enum: [
            'device',
            'storage',
            'encryptedFolder',
            'contacts',
            'fileShares',
            'syncRules',
          ],
        },
        target: { type: 'string' },
      },
      required: ['agentId', 'kind', 'target'],
    },
    result: DONE,
    target: 'none',
    scope: 'guardrails',
    effect: 'write',
    agentCallable: false,
    binding: 'global',
  },
]
