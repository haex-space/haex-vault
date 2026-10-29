// Who may write a mail for a contact's address (spec 063, US1, FR-001, FR-018). Pure: the mail
// button feeds it the extension list, `scripts/check-extensions-openers-menu.ts` tests it.
import type { ExtensionSummary } from '../../types/bindings/ExtensionSummary.ts'

/** The kind of link a mail extension registers for (`openers` of its manifest). */
export const MAIL_COMPOSE = 'mail.compose'

/** What an extension is called in the menu. */
export interface OpenerRow {
  extensionId: string
  title: string
}

/** The enabled extensions that registered for mail links, by name. The list holds the kind only
 * for an extension that is installed and enabled, so a switched-off one is not asked again here. */
export function openerRows(
  extensions: readonly ExtensionSummary[],
): OpenerRow[] {
  return extensions
    .filter((e) => e.enabled && e.openers.includes(MAIL_COMPOSE))
    .map((e) => ({ extensionId: e.id, title: e.title }))
    .sort(
      (a, b) =>
        a.title.localeCompare(b.title) ||
        a.extensionId.localeCompare(b.extensionId),
    )
}

/** What the mail button does: without an opener it opens the system's mail program as it always
 * did; with one it asks first. */
export function mailTargetBehavior(
  rows: readonly OpenerRow[],
): 'system' | 'menu' {
  return rows.length === 0 ? 'system' : 'menu'
}

/** Where a request comes from: the person's tap on the button, or a link from another app. */
export type MailSource = 'button' | 'external'

/** A link from outside never goes to the system's mail program, whatever the person picks next
 * (FR-018): holzi would hand an unvetted link on. */
export function systemAllowed(source: MailSource): boolean {
  return source === 'button'
}
