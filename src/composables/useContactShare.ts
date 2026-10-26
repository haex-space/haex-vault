import { invoke } from '@tauri-apps/api/core'
import type { ContactShareOptions } from '@bindings/ContactShareOptions'
import type { ContactShareOutput } from '@bindings/ContactShareOutput'
import type { ContactShareSelection } from '@bindings/ContactShareSelection'

/**
 * Sharing a contact (spec 060, contracts/tauri-commands.md): thin `invoke` wrappers. The text of a
 * share is built in the backend from the choice; nothing is stored there or here.
 */
export function useContactShare() {
  const shareOptionsAsync = (contactId: string) =>
    invoke<ContactShareOptions>('contacts_share_options', {
      args: { contactId },
    })

  const shareBuildAsync = (
    contactId: string,
    selection: ContactShareSelection,
  ) =>
    invoke<ContactShareOutput>('contacts_share_build', {
      args: { contactId, selection },
    })

  return { shareOptionsAsync, shareBuildAsync }
}
