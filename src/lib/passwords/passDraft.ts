// The draft of the pass editor (spec 092, FR-002, FR-003, FR-012): the form state, and how it turns
// into a create input or into a partial update that sends only what the user changed. The start is
// typed as a wall clock at the place and a zone, and becomes a point in time (UTC) only when it is
// sent. The codes are a list by id: a code that keeps its id keeps its row. Pure, so
// `scripts/check-passwords-pass-draft.ts` runs it without vue.
import { instantToWallTime, wallTimeToInstant } from './passDate.ts'

/** The part of an `ItemDetail` the form starts from (structural, so this module stays loadable by
 * the Node check scripts without the generated bindings). */
export type PassDetailLike = {
  title: string | null
  note: string | null
  url: string | null
  icon: string | null
  color: string | null
  expiresAt: string | null
  passKind?: string | null
  startDate?: string | null
  startAt?: string | null
  startZone?: string | null
  parentId?: string | null
  tags: { name: string }[]
  keyValues: { id: string; key: string | null; value: string | null }[]
}

/** The part of a `CodeView` the form starts from. */
export type CodeLike = {
  id: string
  format: string
  value: string
  label: string | null
  flag?: string | null
}

export type StartDraft =
  | { mode: 'none' }
  | { mode: 'date'; date: string }
  | { mode: 'moment'; wall: string; zone: string }

/** A code in the form; `id` is `null` for a new one. */
export type CodeDraft = {
  id: string | null
  format: string
  value: string
  label: string
}

export type KeyValueDraft = { id: string | null; key: string; value: string }

export type PassDraft = {
  title: string
  note: string
  url: string
  icon: string | null
  color: string | null
  /** `YYYY-MM-DD` or empty: the end of the pass. */
  expiresAt: string
  /** The stored text: a kind this build does not know stays as it is until the user picks one. */
  passKind: string
  start: StartDraft
  /** The entry this pass belongs to, or `null`. */
  parentId: string | null
  codes: CodeDraft[]
  keyValues: KeyValueDraft[]
  /** Tag names. */
  tags: string[]
}

export const CODE_FORMATS = [
  'qr',
  'code128',
  'ean13',
  'pdf417',
  'aztec',
] as const

/** Same shapes as the generated `Start`, `CodeInput`, `PassInput` and `PassPatch`. */
export type StartOut = { date: string } | { at: string; zone: string }
export type CodeOut = {
  id?: string
  format: string
  value: string
  label?: string
  position: number
}
export type PassInputOut = {
  title?: string
  note?: string
  url?: string
  icon?: string
  color?: string
  expiresAt?: string
  passKind: string
  start?: StartOut
  parentId?: string
  codes: CodeOut[]
  keyValues: { key: string; value?: string }[]
  tagNames: string[]
}
export type PassPatchOut = {
  title?: string | null
  note?: string | null
  url?: string | null
  icon?: string | null
  color?: string | null
  expiresAt?: string | null
  passKind?: string
  start?: StartOut | null
  parentId?: string | null
  codes?: CodeOut[]
  keyValues?: { id?: string; key: string; value?: string }[]
  tagNames?: string[]
}

export function emptyPassDraft(): PassDraft {
  return {
    title: '',
    note: '',
    url: '',
    icon: null,
    color: null,
    expiresAt: '',
    passKind: 'other',
    start: { mode: 'none' },
    parentId: null,
    codes: [],
    keyValues: [],
    tags: [],
  }
}

/** The form state of a stored pass and its codes. A moment whose zone cannot be applied opens as
 * no start, never as a wrong one. */
export function passDraftFromDetail(
  detail: PassDetailLike,
  codes: readonly CodeLike[],
): PassDraft {
  let start: StartDraft = { mode: 'none' }
  if (detail.startDate) {
    start = { mode: 'date', date: detail.startDate }
  } else if (detail.startAt && detail.startZone) {
    const wall = instantToWallTime(detail.startAt, detail.startZone)
    if (wall !== null) start = { mode: 'moment', wall, zone: detail.startZone }
  }
  return {
    title: detail.title ?? '',
    note: detail.note ?? '',
    url: detail.url ?? '',
    icon: detail.icon,
    color: detail.color,
    expiresAt: detail.expiresAt ?? '',
    passKind: detail.passKind || 'other',
    start,
    parentId: detail.parentId ?? null,
    codes: codes.map((code) => ({
      id: code.id,
      format: code.format,
      value: code.value,
      label: code.label ?? '',
    })),
    keyValues: detail.keyValues.map((field) => ({
      id: field.id,
      key: field.key ?? '',
      value: field.value ?? '',
    })),
    tags: detail.tags.map((tag) => tag.name),
  }
}

/** A deep copy, so the initial state is not changed by edits. */
export function clonePassDraft(draft: PassDraft): PassDraft {
  return JSON.parse(JSON.stringify(draft)) as PassDraft
}

export function isPassDirty(initial: PassDraft, draft: PassDraft): boolean {
  return JSON.stringify(initial) !== JSON.stringify(draft)
}

const orNull = (text: string): string | null => (text === '' ? null : text)

/** Whether a text is a calendar date `YYYY-MM-DD` that exists (not 30 February). */
function isRealDate(text: string): boolean {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(text)
  if (!m) return false
  const [year, month, day] = [Number(m[1]), Number(m[2]), Number(m[3])]
  const date = new Date(Date.UTC(year, month - 1, day))
  return (
    date.getUTCFullYear() === year &&
    date.getUTCMonth() === month - 1 &&
    date.getUTCDate() === day
  )
}

/** Whether a zone text is one the window can apply: a fixed offset (`+02:00`) or an IANA name the
 * platform knows. */
export function isKnownZone(zone: string): boolean {
  const offset = /^[+-](\d{2}):(\d{2})$/.exec(zone)
  // The same range `passDate.ts` applies; an offset outside it would be read as UTC.
  if (offset) return Number(offset[1]) <= 23 && Number(offset[2]) <= 59
  try {
    new Intl.DateTimeFormat('en', { timeZone: zone })
    return true
  } catch {
    return false
  }
}

/**
 * What the draft's start sends: `undefined` for none, the date, or the moment as a UTC instant with
 * its zone. `'invalid'` for a date or a moment that is not filled in or does not exist, which the
 * editor shows at the field instead of sending.
 */
export function startOut(start: StartDraft): StartOut | undefined | 'invalid' {
  if (start.mode === 'none') return undefined
  if (start.mode === 'date') {
    return isRealDate(start.date) ? { date: start.date } : 'invalid'
  }
  const zone = start.zone.trim()
  const at =
    zone === '' || !isKnownZone(zone)
      ? null
      : wallTimeToInstant(start.wall, zone)
  return at === null ? 'invalid' : { at, zone }
}

/** Whether the start can be sent as it is. */
export function startIsValid(start: StartDraft): boolean {
  return startOut(start) !== 'invalid'
}

/** Whether a code goes in the request: a new one without a value is an empty placeholder and is
 * left out; a stored one without a value is kept, so the backend refuses it instead of the save
 * deleting it without a word. */
function isSent(code: CodeDraft): boolean {
  return code.id !== null || code.value.trim() !== ''
}

/** The codes in order, positions from the list. */
export function codesOut(codes: readonly CodeDraft[]): CodeOut[] {
  return codes.filter(isSent).map((code, position) => ({
    id: code.id ?? undefined,
    format: code.format,
    value: code.value,
    label: orNull(code.label.trim()) ?? undefined,
    position,
  }))
}

/** The input to create a pass: empty texts are left out, so are custom fields without a key. An
 * invalid start is left out too; `startIsValid` keeps the editor from sending one. */
export function toPassInput(
  draft: PassDraft,
  options: { keepCodeIds?: boolean } = {},
): PassInputOut {
  const start = startOut(draft.start)
  const input: PassInputOut = {
    title: orNull(draft.title) ?? undefined,
    note: orNull(draft.note) ?? undefined,
    url: orNull(draft.url) ?? undefined,
    icon: draft.icon ?? undefined,
    color: draft.color ?? undefined,
    expiresAt: orNull(draft.expiresAt) ?? undefined,
    passKind: draft.passKind,
    parentId: draft.parentId ?? undefined,
    // The ids go along only for a copy ("save as new"): the backend then lets a code that arrived
    // flagged keep its flag; a code of another value has to fit its format.
    codes: codesOut(draft.codes).map(({ id, ...code }) =>
      options.keepCodeIds ? { id, ...code } : code,
    ),
    keyValues: draft.keyValues
      .filter((field) => field.key.trim() !== '')
      .map((field) => ({ key: field.key, value: field.value })),
    tagNames: [...draft.tags],
  }
  if (start !== undefined && start !== 'invalid') input.start = start
  return input
}

function sameList(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((value, i) => value === b[i])
}

/** The partial update from `initial` to `draft`: only what changed. The kind is sent only when the
 * user changed it, so a kind this build does not know is written back as it was. */
export function toPassPatch(
  initial: PassDraft,
  draft: PassDraft,
): PassPatchOut {
  const patch: PassPatchOut = {}
  if (draft.title !== initial.title) patch.title = draft.title
  if (draft.note !== initial.note) patch.note = draft.note
  if (draft.url !== initial.url) patch.url = draft.url
  if (draft.icon !== initial.icon) patch.icon = draft.icon
  if (draft.color !== initial.color) patch.color = draft.color
  if (draft.expiresAt !== initial.expiresAt)
    patch.expiresAt = orNull(draft.expiresAt)
  if (draft.passKind !== initial.passKind) patch.passKind = draft.passKind
  if (JSON.stringify(draft.start) !== JSON.stringify(initial.start)) {
    const start = startOut(draft.start)
    if (start === undefined) patch.start = null
    else if (start !== 'invalid') patch.start = start
  }
  if (draft.parentId !== initial.parentId) patch.parentId = draft.parentId
  if (JSON.stringify(draft.codes) !== JSON.stringify(initial.codes)) {
    patch.codes = codesOut(draft.codes)
  }
  if (!sameList([...draft.tags].sort(), [...initial.tags].sort())) {
    patch.tagNames = [...draft.tags]
  }
  if (JSON.stringify(draft.keyValues) !== JSON.stringify(initial.keyValues)) {
    patch.keyValues = draft.keyValues.map((field) => ({
      id: field.id ?? undefined,
      key: field.key,
      value: field.value,
    }))
  }
  return patch
}

/**
 * The index in the draft of the code the backend names by `position`: it counts the codes that are
 * sent, which leaves out new ones without a value (`codesOut`). `null` if there is none.
 */
export function draftIndexOfPosition(
  codes: readonly CodeDraft[],
  position: number,
): number | null {
  let sent = -1
  for (const [index, code] of codes.entries()) {
    if (!isSent(code)) continue
    sent += 1
    if (sent === position) return index
  }
  return null
}
