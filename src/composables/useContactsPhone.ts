import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { ContactsPhoneAccount } from '@bindings/ContactsPhoneAccount'
import type { ContactsPhoneEnabled } from '@bindings/ContactsPhoneEnabled'
import type { ContactsPhoneMassDecision } from '@bindings/ContactsPhoneMassDecision'
import type { ContactsPhoneMassDelete } from '@bindings/ContactsPhoneMassDelete'
import type { ContactsPhonePairDecision } from '@bindings/ContactsPhonePairDecision'
import type { ContactsPhoneReview } from '@bindings/ContactsPhoneReview'
import type { ContactsPhoneRunSummary } from '@bindings/ContactsPhoneRunSummary'
import type { ContactsPhoneStatus } from '@bindings/ContactsPhoneStatus'

/**
 * The sync with the phone's address book (spec 051 US2, contracts/tauri-commands.md): thin
 * `invoke` wrappers and the state the settings show, kept up to date by the backend's events
 * (`contacts-phone-status`, `contacts-phone-mass-delete`). Only on a device with the capability
 * `phoneContacts`; elsewhere the settings say it is not available here.
 */
export function useContactsPhone() {
  const status = shallowRef<ContactsPhoneStatus | null>(null)
  const massDelete = shallowRef<ContactsPhoneMassDelete | null>(null)
  const unlisten: Promise<UnlistenFn>[] = []

  const statusAsync = async () => {
    status.value = await invoke<ContactsPhoneStatus>('contacts_phone_status')
    massDelete.value = status.value.massDelete
    return status.value
  }

  const enableAsync = () =>
    invoke<ContactsPhoneEnabled>('contacts_phone_enable')
  const disableAsync = () => invoke('contacts_phone_disable')
  const accountsAsync = () =>
    invoke<ContactsPhoneAccount[]>('contacts_phone_accounts')
  const setTargetAsync = (account: ContactsPhoneAccount) =>
    invoke('contacts_phone_set_target', { args: { account } })
  const reviewAsync = () => invoke<ContactsPhoneReview>('contacts_phone_review')
  const confirmAsync = (decisions: ContactsPhonePairDecision[]) =>
    invoke<ContactsPhoneRunSummary>('contacts_phone_confirm', {
      args: { decisions },
    })
  const massDeleteAsync = (decision: ContactsPhoneMassDecision) =>
    invoke<ContactsPhoneRunSummary>('contacts_phone_mass_delete', {
      args: { decision },
    })
  const runAsync = () => invoke<ContactsPhoneRunSummary>('contacts_phone_run')

  /** Follows the backend's events while the caller is mounted. */
  function follow() {
    onMounted(() => {
      unlisten.push(
        listen<ContactsPhoneStatus>('contacts-phone-status', (event) => {
          status.value = event.payload
          massDelete.value = event.payload.massDelete
        }),
        listen<ContactsPhoneMassDelete>(
          'contacts-phone-mass-delete',
          (event) => {
            massDelete.value = event.payload
          },
        ),
      )
    })
    onBeforeUnmount(() => {
      for (const stop of unlisten.splice(0)) void stop.then((fn) => fn())
    })
  }

  return {
    status,
    massDelete,
    follow,
    statusAsync,
    enableAsync,
    disableAsync,
    accountsAsync,
    setTargetAsync,
    reviewAsync,
    confirmAsync,
    massDeleteAsync,
    runAsync,
  }
}

/** The key of an account, for a select. */
export function accountKey(account: ContactsPhoneAccount): string {
  return `${account.accountType}\n${account.name}`
}
