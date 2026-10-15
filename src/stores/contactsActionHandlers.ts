import { invoke } from '@tauri-apps/api/core'
import { patchOf } from '~/lib/actions/contactsActions'
import type { useWindowManagerStore } from '~/stores/windowManager'

type WmStore = ReturnType<typeof useWindowManagerStore>

/**
 * The global handlers of the address book actions (spec 051 US6, `lib/actions/contactsActions.ts`):
 * each calls the one backend command that runs as the built-in agent. Rust asks the user for a
 * grant the agent lacks and waits for the answer; a refusal comes back like a missing contact.
 */
export function registerContactsActionHandlers(wm: WmStore): void {
  wm.registerGlobalActionHandler('contacts.search', async ({ input }) => ({
    contacts: await invoke<unknown[]>('contacts_agent_search', {
      args: {
        query: typeof input.query === 'string' ? input.query : '',
        limit: typeof input.limit === 'number' ? input.limit : undefined,
      },
    }),
  }))
  wm.registerGlobalActionHandler('contacts.get', async ({ input }) =>
    invoke<unknown>('contacts_agent_get', { args: { id: String(input.id) } }),
  )
  wm.registerGlobalActionHandler('contacts.update', async ({ input }) =>
    invoke<unknown>('contacts_agent_update', {
      args: { id: String(input.id), patch: patchOf(input) },
    }),
  )
}
