// The kinds of contact values (spec 051, FR-003, FR-007, data-model.md) as the window offers them:
// which kinds the editor knows, their labels and how a stored value reads as text. The checks of a
// value run in Rust (`src-tauri/src/contacts/kinds.rs`); this file only shapes input and output.
// Pure, so `scripts/check-contacts-kinds.ts` runs it without vue.
import { formatEventDate } from './eventDate.ts'

export type KindId =
  | 'phone'
  | 'email'
  | 'address'
  | 'website'
  | 'im'
  | 'sip'
  | 'event'
  | 'relation'
  | 'custom'
  | 'holzi_vault'
  | 'holzi_nostr_relay'
  | 'holzi_iroh'

export type KindDefinition = {
  id: KindId
  icon: string
  /** Labels the kind offers, first is the default; `custom:<text>` is always possible. */
  labels: readonly string[]
  /** How the value is entered. */
  input: 'text' | 'address' | 'im' | 'date'
  /** Only in holzi, never on a phone (FR-014). */
  holziOnly: boolean
}

const COMMON = ['home', 'work', 'other'] as const

export const KINDS: readonly KindDefinition[] = [
  {
    id: 'phone',
    icon: 'lucide:phone',
    labels: ['mobile', ...COMMON, 'main', 'fax_work', 'fax_home', 'pager'],
    input: 'text',
    holziOnly: false,
  },
  {
    id: 'email',
    icon: 'lucide:mail',
    labels: [...COMMON, 'mobile'],
    input: 'text',
    holziOnly: false,
  },
  {
    id: 'address',
    icon: 'lucide:map-pin',
    labels: COMMON,
    input: 'address',
    holziOnly: false,
  },
  {
    id: 'website',
    icon: 'lucide:globe',
    labels: COMMON,
    input: 'text',
    holziOnly: false,
  },
  {
    id: 'im',
    icon: 'lucide:message-circle',
    labels: COMMON,
    input: 'im',
    holziOnly: false,
  },
  {
    id: 'sip',
    icon: 'lucide:phone-call',
    labels: COMMON,
    input: 'text',
    holziOnly: false,
  },
  {
    id: 'event',
    icon: 'lucide:cake',
    labels: ['birthday', 'anniversary', 'other'],
    input: 'date',
    holziOnly: false,
  },
  {
    id: 'relation',
    icon: 'lucide:users',
    labels: [
      'spouse',
      'partner',
      'child',
      'mother',
      'father',
      'parent',
      'sister',
      'brother',
      'relative',
      'friend',
      'manager',
      'assistant',
      'other',
    ],
    input: 'text',
    holziOnly: false,
  },
  {
    id: 'custom',
    icon: 'lucide:tag',
    labels: [],
    input: 'text',
    holziOnly: true,
  },
  {
    id: 'holzi_vault',
    icon: 'lucide:key-round',
    labels: ['other'],
    input: 'text',
    holziOnly: true,
  },
  {
    id: 'holzi_nostr_relay',
    icon: 'lucide:radio-tower',
    labels: ['other'],
    input: 'text',
    holziOnly: true,
  },
  {
    id: 'holzi_iroh',
    icon: 'lucide:network',
    labels: ['other'],
    input: 'text',
    holziOnly: true,
  },
]

const BY_ID = new Map(KINDS.map((kind) => [kind.id, kind]))

export function kindOf(id: string): KindDefinition | undefined {
  return BY_ID.get(id as KindId)
}

/** The kinds whose values are the holzi addresses of a contact (FR-008). */
export const HOLZI_ADDRESS_KINDS: readonly KindId[] = [
  'holzi_vault',
  'holzi_nostr_relay',
  'holzi_iroh',
]

export const ADDRESS_PARTS = [
  'street',
  'pobox',
  'neighborhood',
  'postcode',
  'city',
  'region',
  'country',
] as const

export type AddressParts = Partial<
  Record<(typeof ADDRESS_PARTS)[number], string>
>

const CUSTOM_PREFIX = 'custom:'

/** The text of a label of the user's own, or `null` for a fixed label. */
export function customLabel(label: string): string | null {
  return label.startsWith(CUSTOM_PREFIX)
    ? label.slice(CUSTOM_PREFIX.length)
    : null
}

export function makeCustomLabel(text: string): string {
  return `${CUSTOM_PREFIX}${text.trim()}`
}

function parseObject(value: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(value)
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : null
  } catch {
    return null
  }
}

function text(object: Record<string, unknown> | null, key: string): string {
  const field = object?.[key]
  return typeof field === 'string' ? field.trim() : ''
}

export function parseAddress(value: string): AddressParts {
  const object = parseObject(value)
  const parts: AddressParts = {}
  for (const part of ADDRESS_PARTS) {
    const field = text(object, part)
    if (field) parts[part] = field
  }
  return parts
}

export function serializeAddress(parts: AddressParts): string {
  const kept: AddressParts = {}
  for (const part of ADDRESS_PARTS) {
    const field = parts[part]?.trim()
    if (field) kept[part] = field
  }
  return JSON.stringify(kept)
}

export function parseIm(value: string): { service: string; handle: string } {
  const object = parseObject(value)
  return { service: text(object, 'service'), handle: text(object, 'handle') }
}

export function serializeIm(service: string, handle: string): string {
  return JSON.stringify({ service: service.trim(), handle: handle.trim() })
}

/** The date of an event: `YYYY-MM-DD` or `--MM-DD` (no year). */
export function parseEventDate(value: string): string {
  return text(parseObject(value), 'date')
}

export function serializeEventDate(date: string): string {
  return JSON.stringify({ date: date.trim() })
}

/** A stored value as one line of text for the list and the clipboard; an unknown kind shows its
 * raw value. With a `locale` an event date reads in the order of that language. */
export function displayValue(
  kind: string,
  value: string,
  locale?: string,
): string {
  switch (kind) {
    case 'address': {
      const parts = parseAddress(value)
      const cityLine = [parts.postcode, parts.city].filter(Boolean).join(' ')
      return [
        parts.street,
        parts.pobox,
        parts.neighborhood,
        cityLine,
        parts.region,
        parts.country,
      ]
        .filter(Boolean)
        .join(', ')
    }
    case 'im': {
      const { service, handle } = parseIm(value)
      return service ? `${handle} (${service})` : handle
    }
    case 'event': {
      const date = parseEventDate(value)
      if (locale) return formatEventDate(date, locale)
      return date.startsWith('--') ? date.slice(2) : date
    }
    default:
      return value
  }
}

/** Where a value opens (FR-006): call, message, mail, map or web; empty when it opens nowhere. */
export function openTargets(
  kind: string,
  value: string,
): { action: 'call' | 'message' | 'mail' | 'map' | 'web'; url: string }[] {
  switch (kind) {
    case 'phone': {
      const number = value.replace(/[^\d+]/g, '')
      return number
        ? [
            { action: 'call', url: `tel:${number}` },
            { action: 'message', url: `sms:${number}` },
          ]
        : []
    }
    case 'email':
      return [{ action: 'mail', url: `mailto:${value.trim()}` }]
    case 'address': {
      const line = displayValue(kind, value)
      return line
        ? [{ action: 'map', url: `geo:0,0?q=${encodeURIComponent(line)}` }]
        : []
    }
    case 'website': {
      const url = /^https?:\/\//i.test(value.trim())
        ? value.trim()
        : `https://${value.trim()}`
      return [{ action: 'web', url }]
    }
    default:
      return []
  }
}
