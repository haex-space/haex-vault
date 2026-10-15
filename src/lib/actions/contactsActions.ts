// Address book actions for agents (spec 051 US6, FR-030, FR-032, research R15): search, read and
// change contacts. Each runs one backend command that fixes the built-in agent as the caller
// (`stores/contactsActionHandlers.ts`); without a grant holzi asks the user there, and a refusal
// looks like a missing contact. Agents never change groups, phone targets or where a contact came
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
      'Search the address book by name, organization, phone number or e-mail address. Returns id, name, organization, phone numbers, e-mail addresses and group names of each match. Needs the user to allow reading contacts.',
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
  },
  {
    id: 'contacts.get',
    titleKey: 'actions.contacts.get',
    description:
      'Read one contact with all its values (phone numbers, addresses, dates, …) and their ids. Needs the user to allow reading contacts.',
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
  },
]

const HEAD_KEYS = [
  'givenName',
  'familyName',
  'displayName',
  'organization',
  'jobTitle',
  'note',
] as const

/** The backend's patch (`ContactPatch`) from the input of `contacts.update`: a head field given as
 * text is set, as an empty text cleared; values with an id change, without one they are added. */
export function patchOf(
  input: Record<string, unknown>,
): Record<string, unknown> {
  const patch: Record<string, unknown> = {}
  for (const key of HEAD_KEYS) {
    const value = input[key]
    if (typeof value === 'string') patch[key] = value.trim() ? value : null
  }
  const values = Array.isArray(input.values) ? input.values : []
  const remove = Array.isArray(input.remove)
    ? input.remove.filter((id): id is string => typeof id === 'string')
    : []
  if (values.length > 0 || remove.length > 0) {
    patch.values = {
      upsert: values.flatMap((raw) => {
        const value = raw as Record<string, unknown>
        if (typeof value.kind !== 'string' || typeof value.value !== 'string')
          return []
        return [
          {
            ...(typeof value.id === 'string' ? { id: value.id } : {}),
            kind: value.kind,
            label: typeof value.label === 'string' ? value.label : 'other',
            value: value.value,
          },
        ]
      }),
      remove,
    }
  }
  return patch
}
