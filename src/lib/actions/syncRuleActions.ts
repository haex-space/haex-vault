// Sync rule actions for agents (spec 061 US5, FR-035, contracts/tauri-commands.md): list the rules
// of this device with their status, and create, change or delete one. They run in Rust
// (`runner: 'native'`, `src-tauri/src/file_sync/agent.rs`) under the agent's permission
// "Sync-Regeln verwalten"; every change waits for the user's confirmation in a dialog of its own.
// The window manages rules through its own commands. Pure, relative `.ts` imports only.
import type { ActionDefinition, JsonSchema } from './types.ts'

const PLACE: JsonSchema = {
  type: 'object',
  description:
    "A folder of the file browser: the device with an absolute path, or a storage with its path ('/' for the root).",
  properties: {
    source: {
      type: 'object',
      properties: {
        kind: { type: 'string', enum: ['device', 'storage'] },
        storageId: { type: 'string' },
      },
      required: ['kind'],
    },
    path: { type: 'string' },
  },
  required: ['source', 'path'],
}

const DRAFT: JsonSchema = {
  type: 'object',
  properties: {
    name: { type: 'string', description: '1 to 80 characters.' },
    placeA: {
      ...PLACE,
      description:
        'The source. Its .haex-ignore file holds the exclusions of the rule.',
    },
    placeB: { ...PLACE, description: 'The target.' },
    direction: {
      type: 'string',
      enum: ['aToB', 'both'],
      description:
        'aToB: one way from the source (placeA) to the target (placeB); both: both ways.',
    },
    deleteMode: {
      type: 'string',
      enum: ['mirror', 'keep', 'ask'],
      description:
        'What a deletion on one side does on the other: mirror it into the trash, keep the file, or ask the user.',
    },
    intervalSecs: {
      type: 'integer',
      description: 'How often the rule runs, 60 to 86400 seconds.',
    },
    enabled: { type: 'boolean' },
    allowDecrypted: {
      type: 'boolean',
      description:
        'Files of an encrypted source may be stored decrypted in the target. The user sees it in the confirmation.',
    },
  },
  required: [
    'name',
    'placeA',
    'placeB',
    'direction',
    'deleteMode',
    'intervalSecs',
    'enabled',
    'allowDecrypted',
  ],
}

const VIEW: JsonSchema = {
  type: 'object',
  properties: {
    rule: {
      type: 'object',
      properties: {
        id: { type: 'string' },
        name: { type: 'string' },
        placeA: PLACE,
        placeB: PLACE,
        direction: { type: 'string' },
        deleteMode: { type: 'string' },
        intervalSecs: { type: 'integer' },
        enabled: { type: 'boolean' },
        lastRunAt: { type: 'string', nullable: true },
        lastOutcome: { type: 'string', nullable: true },
        lastError: { type: 'string', nullable: true },
      },
    },
    status: {
      type: 'object',
      properties: {
        state: {
          type: 'string',
          enum: ['active', 'running', 'waiting', 'error', 'paused', 'inactive'],
        },
        reason: { type: 'string', nullable: true },
        pendingDeletes: { type: 'integer' },
        conflicts: { type: 'integer' },
      },
    },
  },
}

const RULE_ID: JsonSchema = { type: 'string', description: 'The rule.' }

/** The fields every sync rule action shares. */
const SHARED = {
  target: 'none',
  scope: 'files.syncRules',
  agentCallable: true,
  runner: 'native',
  binding: 'global',
} as const

export const SYNC_RULE_ACTIONS: readonly ActionDefinition[] = [
  {
    id: 'syncRules.list',
    titleKey: 'actions.syncRules.list',
    description:
      'List the sync rules of this device, each with its places, direction, delete mode and status.',
    input: { type: 'object' },
    result: { type: 'array', items: VIEW },
    effect: 'read',
    ...SHARED,
  },
  {
    id: 'syncRules.status',
    titleKey: 'actions.syncRules.status',
    description: 'One sync rule of this device with its status.',
    input: {
      type: 'object',
      properties: { id: RULE_ID },
      required: ['id'],
    },
    result: VIEW,
    effect: 'read',
    ...SHARED,
  },
  {
    id: 'syncRules.create',
    titleKey: 'actions.syncRules.create',
    description:
      "Create a sync rule that keeps two folders equal. The user confirms every call in a dialog; holzi's own data and contacts' shares cannot be a place.",
    input: DRAFT,
    result: VIEW,
    effect: 'destructive',
    confirmsItself: true,
    ...SHARED,
  },
  {
    id: 'syncRules.update',
    titleKey: 'actions.syncRules.update',
    description:
      'Change a sync rule; the draft replaces all its settings. The user confirms every call in a dialog that shows the old values.',
    input: {
      type: 'object',
      properties: { id: RULE_ID, draft: DRAFT },
      required: ['id', 'draft'],
    },
    result: VIEW,
    effect: 'destructive',
    confirmsItself: true,
    ...SHARED,
  },
  {
    id: 'syncRules.delete',
    titleKey: 'actions.syncRules.delete',
    description:
      'Delete a sync rule; the files of both places stay. The user confirms every call in a dialog.',
    input: {
      type: 'object',
      properties: { id: RULE_ID },
      required: ['id'],
    },
    result: { type: 'object' },
    effect: 'destructive',
    confirmsItself: true,
    ...SHARED,
  },
]
