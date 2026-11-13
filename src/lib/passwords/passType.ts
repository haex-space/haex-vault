// The type of a password-manager entry (spec 092-wallet-passes, FR-001, FR-042): a login, a pass
// (card or ticket) or a type this build does not know because a newer holzi wrote it. Pure, so
// `scripts/check-passwords-pass-type.ts` runs it without vue.

export type EntryType = 'login' | 'pass' | 'unknown'

/** The part of a header the type needs; a header from before the column has no `kind`. */
export type TypedHeader = { kind?: string | null }

/**
 * The type of an entry. A missing or empty `kind` is a login (the entry is from a vault that
 * existed before types); any text that is neither `login` nor `pass` is `unknown`, never a login, so
 * the window does not open an editor that would write over what a newer holzi stored.
 */
export function entryTypeOf(header: TypedHeader | null | undefined): EntryType {
  const kind = header?.kind
  if (kind === undefined || kind === null || kind === '' || kind === 'login') {
    return 'login'
  }
  return kind === 'pass' ? 'pass' : 'unknown'
}

/** Whether the window may edit the entry; an unknown type is shown, never edited or written. */
export function isEditable(header: TypedHeader | null | undefined): boolean {
  return entryTypeOf(header) !== 'unknown'
}

/** The kinds of pass the window offers (spec 092, FR-002), in the order of the picker. */
export const PASS_KINDS = [
  'ticket',
  'loyalty_card',
  'voucher',
  'membership',
  'boarding_pass',
  'other',
] as const
export type PassKind = (typeof PASS_KINDS)[number]

/** The part of a header the kind of a pass needs. */
export type PassKindHeader = { passKind?: string | null }

/**
 * The kind of a pass for display. A missing or unknown text (a kind of a newer holzi, FR-004) reads
 * as `other`; the stored text is not changed by reading it.
 */
export function passKindOf(
  header: PassKindHeader | null | undefined,
): PassKind {
  const kind = header?.passKind
  return PASS_KINDS.find((candidate) => candidate === kind) ?? 'other'
}

/**
 * The icon of each kind, as literal names so the Nuxt icon scan finds them (research R17). It is
 * the entry's own icon that wins; this one is the fallback in lists and views.
 */
export const PASS_KIND_ICONS: Record<PassKind, string> = {
  ticket: 'lucide:ticket',
  loyalty_card: 'lucide:id-card',
  voucher: 'lucide:gift',
  membership: 'lucide:award',
  boarding_pass: 'lucide:plane',
  other: 'lucide:wallet',
}

/** The icon for an entry that has none of its own: the kind's icon for a pass. */
export function passFallbackIcon(
  header: (TypedHeader & PassKindHeader) | null | undefined,
): string | null {
  return entryTypeOf(header) === 'pass'
    ? PASS_KIND_ICONS[passKindOf(header)]
    : null
}
