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
