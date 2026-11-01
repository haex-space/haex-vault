// Address book actions for agents (spec 051 US6, FR-030, FR-032, research R15): search, read and
// change contacts. They run in Rust (`runner: 'native'`, `src-tauri/src/contacts/agent_actions.rs`)
// as the calling agent, the built-in one or an external one (spec 021 US5); without a grant holzi
// asks the user there, and a refusal looks like a missing contact. Agents never change groups, phone targets or where a contact came
// from. Pure, relative `.ts` imports only.
import type { ActionDefinition, JsonSchema } from './types.ts'

const SUMMARY: JsonSchema = {
  type: 'object',
  properties: {
    id: { type: 'string' },
    displayName: { type: 'string' },
    organization: { type: 'string' },
    phones: { type: 'array', items: { type: 'string' } },
    emails: { type: 'array', items: { type: 'string' } },
    groups: {
      type: 'array',
      items: { type: 'string' },
      description: 'Group names.',
    },
    photo: { type: 'string', description: 'Hash of the photo, if any.' },
    isSelf: {
      type: 'boolean',
      description:
        'Present and true only on the contact that is the user themselves; its name is their real name.',
    },
  },
}

const VALUE: JsonSchema = {
  type: 'object',
  properties: {
    id: { type: 'string' },
    kind: {
      type: 'string',
      description:
        'phone, email, address, url, date, im, relation, custom or a kind of a newer holzi.',
    },
    label: { type: 'string' },
    value: { type: 'string' },
  },
}

const CONTACT: JsonSchema = {
  type: 'object',
  properties: {
    id: { type: 'string' },
    displayName: { type: 'string' },
    givenName: { type: 'string' },
    familyName: { type: 'string' },
    middleName: { type: 'string' },
    namePrefix: { type: 'string' },
    nameSuffix: { type: 'string' },
    nickname: { type: 'string' },
    organization: { type: 'string' },
    jobTitle: { type: 'string' },
    note: { type: 'string' },
    values: { type: 'array', items: VALUE },
    groups: {
      type: 'array',
      items: { type: 'string' },
      description: 'Group names.',
    },
    photo: { type: 'string', description: 'Hash of the photo, if any.' },
    holziAddresses: {
      type: 'object',
      description:
        "The confirmed holzi addresses from the contact's business card: vault identity, devices and Nostr relays; absent without a card.",
    },
    isSelf: {
      type: 'boolean',
      description:
        'Present and true only on the contact that is the user themselves; its name is their real name.',
    },
    updatedAt: { type: 'string' },
  },
}

const ID: JsonSchema = { type: 'string', description: 'The contact id.' }

const HEAD_FIELD: JsonSchema = {
  type: 'string',
  description: 'The new text; an empty text clears the field.',
}

export const CONTACTS_ACTIONS: readonly ActionDefinition[] = [
  {
    id: 'contacts.search',
    titleKey: 'actions.contacts.search',
    description:
      'Search the address book by name, organization, phone number or e-mail address. Returns id, name, organization, phone numbers, e-mail addresses and group names of each match. The contact marked isSelf is the user themselves. Needs the user to allow reading contacts.',
    input: {
      type: 'object',
      properties: {
        query: {
          type: 'string',
          description:
            'Text a match contains; case and accents do not matter, phone numbers also match as bare digits.',
        },
        limit: {
          type: 'integer',
          description:
            'At most this many contacts (at most 100, 20 by default).',
        },
      },
      required: ['query'],
    },
    result: {
      type: 'object',
      properties: { contacts: { type: 'array', items: SUMMARY } },
    },
    target: 'none',
    scope: 'contacts.read',
    effect: 'read',
    agentCallable: true,
    binding: 'global',
    runner: 'native',
  },
  {
    id: 'contacts.get',
    titleKey: 'actions.contacts.get',
    description:
      'Read one contact with all its values (phone numbers, addresses, dates, …) and their ids. The contact marked isSelf is the user themselves. Needs the user to allow reading contacts.',
    input: {
      type: 'object',
      properties: { id: ID },
      required: ['id'],
    },
    result: CONTACT,
    target: 'none',
    scope: 'contacts.read',
    effect: 'read',
    agentCallable: true,
    binding: 'global',
    runner: 'native',
  },
  {
    id: 'contacts.update',
    titleKey: 'actions.contacts.update',
    description:
      'Change the name, organization, note or values of one contact. Only the fields given change; a value with an id from contacts.get is changed, one without an id is added, ids in `remove` are removed. The change reaches every device and the phone address books like one made in holzi. Needs the user to allow changing contacts.',
    input: {
      type: 'object',
      properties: {
        id: ID,
        givenName: HEAD_FIELD,
        familyName: HEAD_FIELD,
        displayName: HEAD_FIELD,
        organization: HEAD_FIELD,
        jobTitle: HEAD_FIELD,
        note: HEAD_FIELD,
        values: {
          type: 'array',
          items: VALUE,
          description: 'Values to change (with id) or add (without id).',
        },
        remove: {
          type: 'array',
          items: { type: 'string' },
          description: 'Ids of values to remove.',
        },
      },
      required: ['id'],
    },
    result: CONTACT,
    target: 'none',
    scope: 'contacts.write',
    effect: 'write',
    agentCallable: true,
    binding: 'global',
    runner: 'native',
  },
  {
    id: 'contacts.share',
    titleKey: 'actions.contacts.share',
    description:
      'Turn one contact into a text (a link) that the user can hand to someone else. Data leaves the vault with it, so the user is shown what goes along in a dialog on every call, may change the choice or decline, and you get the text only for what they left chosen; a refusal comes back as denied_by_user. `selection` is only a proposal and cannot reach beyond what the dialog offers: leave it out to propose everything, or name the ids of the values (from contacts.get) and set the head fields you do not want to false. Needs the user to allow reading contacts.',
    input: {
      type: 'object',
      properties: {
        contactId: ID,
        selection: {
          type: 'object',
          description:
            'What you propose to share; anything left out is proposed as shared.',
          properties: {
            values: {
              type: 'array',
              items: { type: 'string' },
              description: 'Ids of the values (contacts.get) that go along.',
            },
            displayName: { type: 'boolean' },
            nickname: { type: 'boolean' },
            organization: { type: 'boolean' },
            jobTitle: { type: 'boolean' },
            note: { type: 'boolean' },
            holzi: {
              type: 'object',
              description:
                'Which confirmed holzi addresses to propose: none, the stored card, or selected own-vault devices and relays.',
              properties: {
                kind: { type: 'string', enum: ['none', 'card', 'own'] },
                devices: {
                  type: 'array',
                  items: { type: 'string' },
                  description: 'Device public keys for the own-vault card.',
                },
                relays: {
                  type: 'array',
                  items: { type: 'string' },
                  description: 'Nostr relay URLs for the own-vault card.',
                },
              },
              required: ['kind'],
            },
          },
        },
      },
      required: ['contactId'],
    },
    result: {
      type: 'object',
      properties: {
        link: {
          type: 'string',
          description: 'The text to hand over (a haex-vault://contact/… link).',
        },
        qrAvailable: {
          type: 'boolean',
          description: 'Whether the text is short enough for a QR code.',
        },
        size: { type: 'integer', description: 'Characters of the text.' },
      },
    },
    target: 'none',
    scope: 'contacts.read',
    effect: 'destructive',
    agentCallable: true,
    runner: 'native',
    confirmsItself: true,
    binding: 'global',
  },
]
