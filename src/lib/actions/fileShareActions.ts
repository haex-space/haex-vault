// File share actions (spec 056 US2, contracts/tauri-commands.md): list, save, revoke and delete
// shares of this vault. The window runs them (`stores/fileShareActionHandlers.ts`); agents may not
// call them until they get a permission of their own (US6, FR-045). Pure, relative `.ts` imports
// only.
import type { ActionDefinition, JsonSchema } from './types.ts'

const SHARE: JsonSchema = {
  type: 'object',
  properties: {
    id: { type: 'string' },
    name: { type: 'string' },
    path: { type: 'string' },
    itemKind: { type: 'string', enum: ['folder', 'file'] },
    grants: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          vaultPubkey: { type: 'string' },
          contactName: { type: 'string' },
          level: { type: 'string', enum: ['read', 'readWrite'] },
        },
      },
    },
    state: {
      type: 'string',
      enum: ['active', 'sourceMissing', 'sourceUnavailable', 'pending'],
    },
  },
}

const SHARES: JsonSchema = { type: 'array', items: SHARE }

const SHARE_LOOKUP: JsonSchema = {
  type: 'object',
  properties: { share: SHARE },
}

const SHARE_ID: JsonSchema = { type: 'string', description: 'The share.' }

/** The fields every share action shares. */
const SHARED = {
  target: 'none',
  scope: 'files.shares',
  agentCallable: false,
  binding: 'global',
} as const

export const FILE_SHARE_ACTIONS: readonly ActionDefinition[] = [
  {
    id: 'files.shares.list',
    titleKey: 'actions.files.shares.list',
    description: 'List the file shares of this vault with their contacts.',
    input: { type: 'object' },
    result: SHARES,
    effect: 'read',
    ...SHARED,
  },
  {
    id: 'files.shares.of',
    titleKey: 'actions.files.shares.of',
    description:
      'The share of a folder or file of this device, if there is one; the result has a share field only when found.',
    input: {
      type: 'object',
      properties: { path: { type: 'string' } },
      required: ['path'],
    },
    result: SHARE_LOOKUP,
    effect: 'read',
    ...SHARED,
  },
  {
    id: 'files.shares.save',
    titleKey: 'actions.files.shares.save',
    description:
      'Share a folder or file of this device with contacts, or change the name and contacts of a share; the grants replace the share’s grants.',
    input: {
      type: 'object',
      properties: {
        shareId: SHARE_ID,
        path: { type: 'string' },
        name: { type: 'string' },
        grants: {
          type: 'array',
          items: {
            type: 'object',
            properties: {
              vaultPubkey: { type: 'string' },
              level: { type: 'string', enum: ['read', 'readWrite'] },
            },
            required: ['vaultPubkey', 'level'],
          },
        },
      },
      required: ['path', 'name', 'grants'],
    },
    result: SHARE,
    effect: 'write',
    ...SHARED,
  },
  {
    id: 'files.shares.revoke',
    titleKey: 'actions.files.shares.revoke',
    description: 'Take a contact’s access to a share away.',
    input: {
      type: 'object',
      properties: { shareId: SHARE_ID, vaultPubkey: { type: 'string' } },
      required: ['shareId', 'vaultPubkey'],
    },
    result: SHARE,
    effect: 'write',
    ...SHARED,
  },
  {
    id: 'files.shares.delete',
    titleKey: 'actions.files.shares.delete',
    description: 'Delete a share; its contacts lose access.',
    input: {
      type: 'object',
      properties: { shareId: SHARE_ID },
      required: ['shareId'],
    },
    result: { type: 'object' },
    effect: 'destructive',
    ...SHARED,
  },
  {
    id: 'files.shares.ofContact',
    titleKey: 'actions.files.shares.ofContact',
    description: 'The shares a contact has access to.',
    input: {
      type: 'object',
      properties: { contactId: { type: 'string' } },
      required: ['contactId'],
    },
    result: SHARES,
    effect: 'read',
    ...SHARED,
  },
]
