import { invoke } from '@tauri-apps/api/core'
import type { ContactsOnPhone } from '@bindings/ContactsOnPhone'
import type { ContactsPhoneAccount } from '@bindings/ContactsPhoneAccount'
import type { ContactsPhoneSummary } from '@bindings/ContactsPhoneSummary'

/**
 * Sync targets of contacts (spec 051 FR-018b to FR-018d, contracts/tauri-commands.md
 * §Abgleichsziele): the phones of the vault with the sync on, and setting and removing a phone as
 * a target of contacts or of a group. Works on every device.
 */
export function useContactTargets() {
  const phonesAsync = () => invoke<ContactsPhoneSummary[]>('contacts_phones')

  /** Makes a phone a target of contacts; without an account, of the phone's target account. */
  const targetsSetAsync = (
    contactIds: string[],
    device: string,
    account?: ContactsPhoneAccount,
  ) =>
    invoke('contacts_targets_set', {
      args: account ? { contactIds, device, account } : { contactIds, device },
    })

  /** Makes a phone a target of every contact of a group. */
  const groupTargetSetAsync = (groupId: string, device: string) =>
    invoke('contacts_targets_set', { args: { groupId, device } })

  /** Removes a phone as a target: the phone deletes the contacts or keeps them unsynced. */
  const targetsRemoveAsync = (
    contactIds: string[],
    device: string,
    onPhone: ContactsOnPhone,
  ) =>
    invoke('contacts_targets_remove', {
      args: { contactIds, device, onPhone },
    })

  return {
    phonesAsync,
    targetsSetAsync,
    groupTargetSetAsync,
    targetsRemoveAsync,
  }
}

/** How an account of a phone is named: its name and type, or the phone itself without one. */
export function useAccountLabel() {
  const { t } = useI18n()
  return (accountType: string, name: string) =>
    accountType
      ? `${name} (${accountType})`
      : t('settings.contacts.phone.localAccount')
}
