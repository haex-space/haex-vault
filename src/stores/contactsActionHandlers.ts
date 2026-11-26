import { invoke } from '@tauri-apps/api/core'
import type { useWindowManagerStore } from '~/stores/windowManager'

type WmStore = ReturnType<typeof useWindowManagerStore>

/**
 * Global handlers of blocking and allowing again (spec 099 FR-014, FR-016,
 * `lib/actions/contactsActions.ts`): the conversation, the contact page and
 * the settings call the same act.
 */
export function registerContactsActionHandlers(wm: WmStore): void {
  const done = { done: true }
  const on = wm.registerGlobalActionHandler

  on('contacts.blockContact', async ({ input }) => {
    await invoke('contacts_block_contact', {
      args: { contactId: String(input.contactId) },
    })
    return done
  })
  on('contacts.unblock', async ({ input }) => {
    await invoke('contacts_unblock', {
      args: { vaultPubkey: String(input.vaultPubkey) },
    })
    return done
  })
}
