import { invoke } from '@tauri-apps/api/core'
import type { Contact } from '@bindings/Contact'
import type { ContactShareDecision } from '@bindings/ContactShareDecision'
import type { ContactShareOptions } from '@bindings/ContactShareOptions'
import type { ContactShareOutput } from '@bindings/ContactShareOutput'
import type { ContactSharePreview } from '@bindings/ContactSharePreview'
import type { ContactShareSelection } from '@bindings/ContactShareSelection'
import type { ContactShareTarget } from '@bindings/ContactShareTarget'

/**
 * Sharing a contact and reading a share in (spec 060, contracts/tauri-commands.md): thin `invoke`
 * wrappers. The text of a share is built in the backend from the choice, and a share read in is
 * checked and compared there, on the preview and again on accepting; nothing is stored by a
 * preview.
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

  const sharePreviewAsync = (text: string, target?: ContactShareTarget) =>
    invoke<ContactSharePreview>('contacts_share_preview', {
      args: target ? { text, target } : { text },
    })

  /** Reads a share in; `previewRevision` is the one of the preview the person saw. */
  const shareAcceptAsync = (
    text: string,
    target: ContactShareTarget,
    decisions: ContactShareDecision[],
    previewRevision?: string,
  ) =>
    invoke<Contact>('contacts_share_accept', {
      args: previewRevision
        ? { text, target, decisions, previewRevision }
        : { text, target, decisions },
    })

  return {
    shareOptionsAsync,
    shareBuildAsync,
    sharePreviewAsync,
    shareAcceptAsync,
  }
}
