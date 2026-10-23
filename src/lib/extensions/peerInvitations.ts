// The queue of invitations that holzi's prompt asks about (spec 057 US1, T043): Rust sends
// `extension-peer-invitation` for each and `extension-peer-invitation-gone` when one goes away.
// Pure: the store moves it, `scripts/check-extensions-peer-invitations.ts` tests it. One
// invitation is shown at a time, the oldest first.
import type { ExtensionPeerInvitation } from '../../types/bindings/ExtensionPeerInvitation.ts'

/** Adds an invitation; one that waits already (the same id, shown again after a click on its
 * notification) stays where it is and takes the newer content. */
export function enqueue(
  queue: readonly ExtensionPeerInvitation[],
  invitation: ExtensionPeerInvitation,
): ExtensionPeerInvitation[] {
  const known = queue.some((i) => i.inviteId === invitation.inviteId)
  return known
    ? queue.map((i) => (i.inviteId === invitation.inviteId ? invitation : i))
    : [...queue, invitation]
}

/** Takes an invitation out: answered, dismissed, or gone (withdrawn, expired). */
export function remove(
  queue: readonly ExtensionPeerInvitation[],
  inviteId: string,
): ExtensionPeerInvitation[] {
  return queue.filter((i) => i.inviteId !== inviteId)
}

/** The invitation asked about now. */
export function current(
  queue: readonly ExtensionPeerInvitation[],
): ExtensionPeerInvitation | undefined {
  return queue[0]
}
