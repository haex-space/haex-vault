// Password manager action catalog (spec 034-password-manager, FR-027, contracts/access.md; spec 021
// US5, research R13). `passwords.items.search` returns the id, title, tag names, folder name and
// whether a TOTP exists of the matching entries, and nothing else: no username, no address, no
// secret. The built-in agent searches every entry, an external agent only those its grant covers.
// `passwords.secret.get` hands one secret of one entry to an external agent, after the user
// confirmed this very call; the built-in agent never gets it. Both run in Rust
// (`src-tauri/src/passwords/agent_actions.rs`), which checks every call against the grants, so this
// catalog is not the only guard. Pure, relative `.ts` imports only.
import type { ActionDefinition, JsonSchema } from './types.ts'

const SEARCH_INPUT: JsonSchema = {
  type: 'object',
  properties: {
    query: {
      type: 'string',
      description:
        'Words to look for in the title, the tag names and the folder name of an entry; every word must match.',
    },
    tag: {
      type: 'string',
      description: 'Only entries that carry this tag.',
    },
    limit: {
      type: 'integer',
      description: 'At most this many entries (at most 50, 20 by default).',
    },
  },
}

const SEARCH_RESULT: JsonSchema = {
  type: 'object',
  properties: {
    items: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          id: { type: 'string' },
          title: {
            type: 'string',
            description: 'Absent for an entry without a title.',
          },
          tags: { type: 'array', items: { type: 'string' } },
          folder: { type: 'string', description: 'Absent at the top level.' },
          hasTotp: { type: 'boolean' },
        },
      },
    },
  },
}

export const PASSWORDS_ACTIONS: readonly ActionDefinition[] = [
  {
    id: 'passwords.items.search',
    titleKey: 'actions.passwords.items.search',
    description:
      'Search the password manager by title, tag name or folder name. Returns id, title, tag names, folder name and whether the entry has a one-time code. It never returns usernames, addresses or any secret, and there is no way to read one.',
    input: SEARCH_INPUT,
    result: SEARCH_RESULT,
    target: 'none',
    scope: 'passwords.read',
    effect: 'read',
    agentCallable: true,
    binding: 'global',
    runner: 'native',
  },
  {
    id: 'passwords.secret.get',
    titleKey: 'actions.passwords.secret.get',
    description:
      'Read one secret of one password entry: its password, its current one-time code, or one of its custom fields. The user confirms every single call, whatever the approval mode, and the value goes to you and your model provider. Use passwords.items.search to find the entry id.',
    input: {
      type: 'object',
      properties: {
        entryId: {
          type: 'string',
          description: 'The id of the entry, from passwords.items.search.',
        },
        field: {
          type: 'string',
          description:
            'password, totp (the current one-time code) or custom:<name> for a custom field of that name.',
        },
      },
      required: ['entryId', 'field'],
    },
    result: {
      type: 'object',
      properties: {
        value: {
          type: 'string',
          description: 'The password or the custom field.',
        },
        code: { type: 'string', description: 'The current one-time code.' },
        remainingSeconds: {
          type: 'integer',
          description: 'How long the one-time code stays valid.',
        },
      },
    },
    target: 'none',
    scope: 'passwords.secret',
    effect: 'read',
    agentCallable: true,
    builtinAgentCallable: false,
    binding: 'global',
    runner: 'native',
  },
]
