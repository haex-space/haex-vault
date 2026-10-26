// The own contact "Ich" (spec 060, FR-001 to FR-005): the contact of the vault's user, the same one
// on every device. It always reads "Ich" in the window, whatever its name parts are; the name parts
// themselves stay what the user entered, and search, initials and agents work with them. Pure, so
// `scripts/check-contacts-self.ts` runs it without vue.

/** The id of the own contact, the same in every vault (`SELF_CONTACT_ID` in Rust; never changes). */
export const SELF_CONTACT_ID = '1ce52853-b579-4ffd-b9ef-f27071a414ed'

/** What the window needs of a contact to title it; a `ContactHeader` and a `Contact` satisfy it. */
export type TitledContact = { isSelf: boolean; name: string }

/** The words the title is made of, translated by the caller. */
export type TitleLabels = { self: string; noName: string }

/** The name the window shows: "Ich" for the own contact, else the name or "no name". */
export function contactTitle(
  contact: TitledContact,
  labels: TitleLabels,
): string {
  if (contact.isSelf) return labels.self
  return contact.name || labels.noName
}

/** The own contact apart from the others, which keep their order. */
export function splitSelf<T extends { isSelf: boolean }>(
  contacts: readonly T[],
): { own: T | null; others: T[] } {
  return {
    own: contacts.find((contact) => contact.isSelf) ?? null,
    others: contacts.filter((contact) => !contact.isSelf),
  }
}

/** The own contact first, whatever its name; the others keep their order. */
export function selfFirst<T extends { isSelf: boolean }>(
  contacts: readonly T[],
): T[] {
  const { own, others } = splitSelf(contacts)
  return own ? [own, ...others] : others
}
