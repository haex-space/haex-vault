import { invoke } from '@tauri-apps/api/core'
import type { Contact } from '@bindings/Contact'
import type { ContactsBlockedVault } from '@bindings/ContactsBlockedVault'
import type { ContactsCardInto } from '@bindings/ContactsCardInto'
import type { ContactsCardPreview } from '@bindings/ContactsCardPreview'
import type { ContactsOwnCard } from '@bindings/ContactsOwnCard'

/**
 * Visitenkarten and blocked vaults (spec 051 US4, contracts/tauri-commands.md §Visitenkarten):
 * thin `invoke` wrappers. Every card is checked in the backend, on the preview and again on
 * accepting.
 */
export function useContactCards() {
  const ownCardAsync = () => invoke<ContactsOwnCard>('contacts_own_card')

  const ownCardSetAsync = (ownContact: string | null, valueIds: string[]) =>
    invoke<ContactsOwnCard>('contacts_own_card_set', {
      args: ownContact ? { ownContact, valueIds } : { valueIds },
    })

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
    ownCardAsync,
    ownCardSetAsync,
    previewAsync,
    acceptAsync,
    blockedAsync,
    blockAsync,
    unblockAsync,
  }
}
