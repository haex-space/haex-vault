import { invoke } from '@tauri-apps/api/core'
import type { ContactsBlockedVault } from '@bindings/ContactsBlockedVault'

/**
 * Blocked vaults (spec 051 FR-039, spec 099 FR-014 to FR-016). Blocking a contact and allowing a
 * vault again run as the user's actions `contacts.blockContact` and `contacts.unblock`. Sharing a
 * contact and reading a share in is `useContactShare` (spec 060).
 */
export function useContactCards() {
  const wm = useWindowManagerStore()

  const blockedAsync = () => invoke<ContactsBlockedVault[]>('contacts_blocked')

  async function runUserAction(id: string, input: Record<string, unknown>) {
    const outcome = await wm.runAction(id, input, { kind: 'user' })
    if (outcome.ok) return
    throw outcome.error !== undefined
      ? outcome.error
      : new Error(outcome.message)
  }

  /** Blocks a vault from a share's preview (spec 051); a vault a contact holds blocks the contact. */
  const blockAsync = (vaultPubkey: string) =>
    invoke('contacts_block', { args: { vaultPubkey } })

  const blockContactAsync = (contactId: string) =>
    runUserAction('contacts.blockContact', { contactId })

  const unblockAsync = (vaultPubkey: string) =>
    runUserAction('contacts.unblock', { vaultPubkey })

  return {
    blockedAsync,
    blockAsync,
    blockContactAsync,
    unblockAsync,
  }
}
