// The editor's draft of a contact (spec 051, FR-002, FR-003, research R3): built from a stored
// contact, edited in the window, turned into a create input or into a patch that names only what
// changed, so a value of a kind this version does not know is never written. Pure, so
// `scripts/check-contacts-draft.ts` runs it without vue.
import { kindOf } from './kinds.ts'

export const HEAD_FIELDS = [
  'namePrefix',
  'givenName',
  'middleName',
  'familyName',
  'nameSuffix',
  'nickname',
  'displayName',
  'organization',
  'jobTitle',
  'note',
] as const

export type HeadField = (typeof HEAD_FIELDS)[number]

// The shapes of the generated bindings (`Contact`, `ContactInput`, `ContactPatch`) as far as the
// draft uses them; declared here so the check scripts load this file without the bindings.
type ContactHead = Partial<Record<HeadField, string>>

type StoredContact = {
  head: ContactHead
  values: readonly {
    id: string
    kind: string
    label: string
    value: string
    known: boolean
  }[]
  groups: readonly { id: string }[]
}

type ContactInput = {
  head: ContactHead
  values: { kind: string; label: string; value: string }[]
  groups: string[]
}

type ContactValueUpsert = {
  id?: string
  kind: string
  label: string
  value: string
}

type ContactPatch = Partial<Record<HeadField, string | null>> & {
  values?: { upsert: ContactValueUpsert[]; remove: string[] }
  groups?: string[]
}

export type DraftValue = {
  /** Absent for a value added in the editor. */
  id?: string
  kind: string
  label: string
  value: string
  /** `false` for a kind this version does not know: shown, never written. */
  known: boolean
}

export type Draft = {
  head: Record<HeadField, string>
  values: DraftValue[]
  groups: string[]
}

export function emptyDraft(groups: string[] = []): Draft {
  const head = Object.fromEntries(
    HEAD_FIELDS.map((field) => [field, '']),
  ) as Record<HeadField, string>
  return { head, values: [], groups }
}

export function draftOf(contact: StoredContact): Draft {
  const head = emptyDraft().head
  for (const field of HEAD_FIELDS) head[field] = contact.head[field] ?? ''
  return {
    head,
    values: contact.values.map((value) => ({
      id: value.id,
      kind: value.kind,
      label: value.label,
      value: value.value,
      known: value.known,
    })),
    groups: contact.groups.map((group) => group.id),
  }
}

/** A new value of a kind with its default label. */
export function newValue(kind: string): DraftValue {
  const definition = kindOf(kind)
  return {
    kind,
    label: definition?.labels[0] ?? 'custom:',
    value: '',
    known: true,
  }
}

function headOf(draft: Draft): ContactHead {
  const head: ContactHead = {}
  for (const field of HEAD_FIELDS) {
    const text = draft.head[field].trim()
    if (text) head[field] = draft.head[field]
  }
  return head
}

/** Values that carry something; empty rows of the editor are dropped. */
function filled(values: readonly DraftValue[]): DraftValue[] {
  return values.filter((value) => value.known && value.value.trim() !== '')
}

export function toInput(draft: Draft): ContactInput {
  return {
    head: headOf(draft),
    values: filled(draft.values).map(({ kind, label, value }) => ({
      kind,
      label,
      value,
    })),
    groups: [...draft.groups],
  }
}

/** The patch from `before` to `draft`: changed head fields (empty clears), changed and new values,
 * removed known values, and the groups when they changed. `null` when nothing changed. */
export function toPatch(before: Draft, draft: Draft): ContactPatch | null {
  const patch: ContactPatch = {}
  let changed = false
  for (const field of HEAD_FIELDS) {
    if (draft.head[field] === before.head[field]) continue
    patch[field] = draft.head[field].trim() ? draft.head[field] : null
    changed = true
  }
  const kept = filled(draft.values)
  const keptIds = new Set(kept.flatMap((value) => (value.id ? [value.id] : [])))
  const old = new Map(before.values.map((value) => [value.id, value]))
  const upsert: ContactValueUpsert[] = []
  for (const value of kept) {
    const stored = value.id ? old.get(value.id) : undefined
    if (stored && stored.label === value.label && stored.value === value.value)
      continue
    upsert.push({
      ...(value.id ? { id: value.id } : {}),
      kind: value.kind,
      label: value.label,
      value: value.value,
    })
  }
  const remove = before.values
    .filter((value) => value.known && value.id && !keptIds.has(value.id))
    .map((value) => value.id as string)
  if (upsert.length || remove.length) {
    patch.values = { upsert, remove }
    changed = true
  }
  const sameGroups =
    before.groups.length === draft.groups.length &&
    before.groups.every((group) => draft.groups.includes(group))
  if (!sameGroups) {
    patch.groups = [...draft.groups]
    changed = true
  }
  return changed ? patch : null
}
