import { invoke } from '@tauri-apps/api/core'
import type { Contact } from '@bindings/Contact'
import type { ContactsBlockedVault } from '@bindings/ContactsBlockedVault'
import type { ContactsCardInto } from '@bindings/ContactsCardInto'
import type { ContactsCardPreview } from '@bindings/ContactsCardPreview'

/**
 * Reading Visitenkarten in and blocked vaults (spec 051 US4, contracts/tauri-commands.md
 * §Visitenkarten): thin `invoke` wrappers. Every card is checked in the backend, on the preview and
 * again on accepting. Sharing a contact is `useContactShare` (spec 060).
 */
export function useContactCards() {
  const previewAsync = (text: string) =>
    invoke<ContactsCardPreview>('contacts_card_preview', { args: { text } })

  const acceptAsync = (text: string, into: ContactsCardInto) =>
    invoke<Contact>('contacts_card_accept', { args: { text, into } })

  const blockedAsync = () => invoke<ContactsBlockedVault[]>('contacts_blocked')

  const blockAsync = (vaultPubkey: string) =>
    invoke('contacts_block', { args: { vaultPubkey } })

  const unblockAsync = (vaultPubkey: string) =>
    invoke('contacts_unblock', { args: { vaultPubkey } })

  return {
    previewAsync,
    acceptAsync,
    blockedAsync,
    blockAsync,
    unblockAsync,
  }
}
