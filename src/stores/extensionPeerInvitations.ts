import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { defineStore } from 'pinia'
import type { ExtensionPeerInvitation } from '@bindings/ExtensionPeerInvitation'
import type { ExtensionPeerInvitationAnswer } from '@bindings/ExtensionPeerInvitationAnswer'
import { current, enqueue, remove } from '~/lib/extensions/peerInvitations'

/**
 * The invitations of contacts that holzi asks the user about (spec 057, US1, T043): Rust sends
 * `extension-peer-invitation` and, when one goes away, `extension-peer-invitation-gone`;
 * `useExtensionHost` listens and calls {@link add} and {@link gone}. One invitation is shown at a
 * time. The answer is only ever given here, never by the extension (FR-016).
 */
export const useExtensionPeerInvitationsStore = defineStore(
  'extensionPeerInvitations',
  () => {
    const queue = ref<ExtensionPeerInvitation[]>([])
    const shown = computed(() => current(queue.value))

    function add(invitation: ExtensionPeerInvitation): void {
      queue.value = enqueue(queue.value, invitation)
    }

    /** The invitation went away: withdrawn, expired, or ended on this device. */
    function gone(inviteId: string): void {
      queue.value = remove(queue.value, inviteId)
    }

    /**
     * Answers the invitation. `'gone'` means it was no longer open (withdrawn, expired, or taken
     * by another device); either way it leaves the queue once Rust has answered. A failed call
     * leaves it there to try again.
     */
    async function answerAsync(
      inviteId: string,
      accept: boolean,
    ): Promise<'open' | 'gone'> {
      const { result } = await invoke<ExtensionPeerInvitationAnswer>(
        'extension_peer_invitation_answer',
        { inviteId, accept },
      )
      queue.value = remove(queue.value, inviteId)
      return result
    }

    /** Closing the prompt decides nothing: the invitation stays open until it expires. */
    function dismiss(inviteId: string): void {
      queue.value = remove(queue.value, inviteId)
    }

    return { queue, shown, add, gone, answerAsync, dismiss }
  },
)
