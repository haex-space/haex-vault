// ---------------------------------------------------------------------------
// Space Types
// ---------------------------------------------------------------------------

export const SpaceType = {
  VAULT: 'vault',
  ONLINE: 'online',
  LOCAL: 'local',
} as const

export type SpaceType = (typeof SpaceType)[keyof typeof SpaceType]

// ---------------------------------------------------------------------------
// Space Status
// ---------------------------------------------------------------------------

export const SpaceStatus = {
  ACTIVE: 'active',
  PENDING: 'pending',
  /**
   * Self-leave is in flight. The membership row + UCAN tokens have been
   * locally deleted (their delete-log entries are in haex_deleted_rows),
   * but we keep the haex_spaces row alive so the per-space sync loop can
   * push those entries to the leader on the next online window. Once
   * delivery is confirmed (or after a give-up window), the row is removed.
   */
  LEAVING: 'leaving',
} as const

export type SpaceStatus = (typeof SpaceStatus)[keyof typeof SpaceStatus]

// ---------------------------------------------------------------------------
// Invite Status
// ---------------------------------------------------------------------------

export const InviteStatus = {
  PENDING: 'pending',
  ACCEPTED: 'accepted',
  DECLINED: 'declined',
} as const

export type InviteStatus = (typeof InviteStatus)[keyof typeof InviteStatus]

// ---------------------------------------------------------------------------
// Outbox Status
// ---------------------------------------------------------------------------

export const OutboxStatus = {
  PENDING: 'pending',
  DELIVERED: 'delivered',
  EXPIRED: 'expired',
  /** Max retries reached without a successful delivery — user intervention required. */
  FAILED: 'failed',
} as const

export type OutboxStatus = (typeof OutboxStatus)[keyof typeof OutboxStatus]

// ---------------------------------------------------------------------------
// Space Capabilities
// ---------------------------------------------------------------------------

export const SpaceCapability = {
  READ: 'space/read',
  WRITE: 'space/write',
  INVITE: 'space/invite',
} as const

export type SpaceCapability = (typeof SpaceCapability)[keyof typeof SpaceCapability]
