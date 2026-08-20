/**
 * Pure helpers for the realtime auto-finalize loop.
 *
 * Kept in its own import-free module so the decision below can be unit-tested
 * without standing up the WebSocket/Tauri/Pinia pipeline that realtime.ts
 * needs — the same split as `computeOutboxNextState` in useInviteOutbox.
 */

/** One row of GET /spaces/:spaceId/invites. */
export type ListedInvite = {
  id: string
  status: string
  /** Present only when this invite is addressed to the caller. */
  ucan?: string | null
  /** Whether the server holds a UCAN for this invite, regardless of addressee. */
  hasUcan?: boolean
  capability?: string
  inviteeDid: string
}

/**
 * Whether finalizing this invite has to mint a fresh UCAN.
 *
 * A UCAN is bearer-usable, so the server emits `ucan` only to the invite's own
 * addressee and reports mere existence through `hasUcan`. The auto-finalize
 * loop runs on the *inviter's* device over invites addressed to other members —
 * exactly the redacted rows — so the presence flag is what it must read.
 * Reading the value instead would see `undefined` for every such row and
 * re-mint a UCAN that already exists.
 *
 * The `??` fallback keeps this correct against a server predating `hasUcan`,
 * since client and server ship as separate PRs and either merge order briefly
 * runs one without the other.
 */
export function inviteNeedsUcan(invite: Pick<ListedInvite, 'ucan' | 'hasUcan'>): boolean {
  return !(invite.hasUcan ?? invite.ucan)
}
