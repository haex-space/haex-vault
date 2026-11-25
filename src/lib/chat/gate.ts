/**
 * What the sender's conversation says while a message waits for the other vault (spec 099 FR-005a,
 * FR-010). The device cannot tell "offline" from "not accepted yet", so the hint names both, and
 * only when nothing ever came back from that vault and a message has waited a minute.
 */

/** How long an own message waits without _delivered_ before the hint shows (FR-005a). */
export const HINT_AFTER_MS = 60_000

/** The fields of a conversation the hint needs. */
export interface GateConversation {
  kind: string
  state: string
  peerAnswered: boolean
  blockedByPeer: boolean
}

/** The fields of a message the hint needs. */
export interface GateMessage {
  own: boolean
  sentAt: number
  delivery: string | null
}

/** Whether the conversation shows "offline or not accepted yet" at `now`. */
export function showsNotYetAccepted(
  conversation: GateConversation,
  messages: readonly GateMessage[],
  now: number,
): boolean {
  if (conversation.kind !== 'direct' || conversation.state === 'request') {
    return false
  }
  if (conversation.peerAnswered || conversation.blockedByPeer) return false
  return messages.some(
    (message) =>
      message.own &&
      message.delivery !== 'delivered' &&
      message.delivery !== 'read' &&
      now - message.sentAt >= HINT_AFTER_MS,
  )
}
