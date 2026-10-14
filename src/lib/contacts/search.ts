// Address book search (spec 051, FR-004, research R11): a contact matches when its name, name
// parts, organization, a phone number or an e-mail address contains the query, without regard to
// case or accents; a query of only phone characters also matches the numbers as bare digits, so
// "01701234567" finds "0170 123-45 67". The same rule runs in Rust
// (`src-tauri/src/contacts/search.rs`); both are checked against `search-cases.json`. Pure, so
// `scripts/check-contacts-search.ts` runs it without vue.
import { fold } from '../search/fold.ts'

/** What the search looks at; a `ContactHeader` satisfies it. */
export type SearchableContact = {
  id: string
  name: string
  givenName: string | null
  familyName: string | null
  organization: string | null
  phones: readonly string[]
  emails: readonly string[]
}

const PHONE_LIKE = /^[\d\s+\-()./]+$/

function digits(text: string): string {
  return text.replace(/\D/g, '')
}

/** The folded text of a contact a query is compared with, one field per line, with the phone
 * numbers once more as bare digits. */
export function haystack(contact: SearchableContact): string {
  const fields = [
    contact.name,
    contact.givenName,
    contact.familyName,
    contact.organization,
    ...contact.phones,
    ...contact.emails,
  ].filter((field): field is string => field !== null)
  return [...fields.map(fold), ...contact.phones.map(digits)].join('\n')
}

/** Whether a contact with this haystack matches `query`; an empty query matches everything. */
export function matchesQuery(text: string, query: string): boolean {
  const folded = fold(query.trim())
  if (!folded || text.includes(folded)) return true
  const bare = digits(folded)
  return PHONE_LIKE.test(folded) && bare.length > 0 && text.includes(bare)
}

/** The contacts that match, in their order. */
export function filterContacts<T extends SearchableContact>(
  contacts: readonly T[],
  query: string,
): T[] {
  if (!query.trim()) return [...contacts]
  return contacts.filter((contact) => matchesQuery(haystack(contact), query))
}

const collator = new Intl.Collator()

/** By folded name; contacts without a name last, then by id. */
export function sortByName<T extends { id: string; name: string }>(
  contacts: readonly T[],
): T[] {
  const keyed = contacts.map((contact) => ({
    contact,
    key: fold(contact.name),
  }))
  keyed.sort((a, b) => {
    if (!a.key !== !b.key) return a.key ? -1 : 1
    return (
      collator.compare(a.key, b.key) || a.contact.id.localeCompare(b.contact.id)
    )
  })
  return keyed.map((entry) => entry.contact)
}
