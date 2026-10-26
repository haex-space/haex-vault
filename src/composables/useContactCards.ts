import { invoke } from '@tauri-apps/api/core'
import type { ContactsBlockedVault } from '@bindings/ContactsBlockedVault'

/**
 * Blocked vaults (spec 051 FR-039): thin `invoke` wrappers. Sharing a contact and reading a share in
 * is `useContactShare` (spec 060).
 */
export function useContactCards() {
  const blockedAsync = () => invoke<ContactsBlockedVault[]>('contacts_blocked')

  const blockAsync = (vaultPubkey: string) =>
    invoke('contacts_block', { args: { vaultPubkey } })

  const unblockAsync = (vaultPubkey: string) =>
    invoke('contacts_unblock', { args: { vaultPubkey } })

  return {
    blockedAsync,
    blockAsync,
    unblockAsync,
  }
}
