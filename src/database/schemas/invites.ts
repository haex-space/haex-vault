import { sql } from 'drizzle-orm'
import {
  integer,
  sqliteTable,
  text,
} from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'

// ---------------------------------------------------------------------------
// Pending Invites — incoming space invitations awaiting user response
// ---------------------------------------------------------------------------

export const haexPendingInvites = sqliteTable(
  tableNames.haex.pending_invites.name,
  {
    id: text(tableNames.haex.pending_invites.columns.id).primaryKey(),
    spaceId: text(tableNames.haex.pending_invites.columns.spaceId).notNull(),
    spaceName: text('space_name'),
    spaceType: text('space_type'),
    originUrl: text('origin_url'),
    inviterDid: text(tableNames.haex.pending_invites.columns.inviterDid).notNull(),
    inviterLabel: text(tableNames.haex.pending_invites.columns.inviterLabel),
    capabilities: text(tableNames.haex.pending_invites.columns.capabilities), // JSON array: ["space/read", "space/write"]
    includeHistory: integer(tableNames.haex.pending_invites.columns.includeHistory, { mode: 'boolean' }).default(false),
    tokenId: text(tableNames.haex.pending_invites.columns.tokenId),
    spaceEndpoints: text(tableNames.haex.pending_invites.columns.spaceEndpoints), // JSON array of EndpointId strings
    status: text(tableNames.haex.pending_invites.columns.status).notNull().default('pending'),
    createdAt: text(tableNames.haex.pending_invites.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
    respondedAt: text(tableNames.haex.pending_invites.columns.respondedAt),
  },
)
export type InsertHaexPendingInvites = typeof haexPendingInvites.$inferInsert
export type SelectHaexPendingInvites = typeof haexPendingInvites.$inferSelect

// ---------------------------------------------------------------------------
// Blocked DIDs — permanently blocked identities
// ---------------------------------------------------------------------------

export const haexBlockedDids = sqliteTable(
  tableNames.haex.blocked_dids.name,
  {
    id: text(tableNames.haex.blocked_dids.columns.id).primaryKey(),
    did: text(tableNames.haex.blocked_dids.columns.did).notNull().unique(),
    label: text(tableNames.haex.blocked_dids.columns.label),
    blockedAt: text(tableNames.haex.blocked_dids.columns.blockedAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
)
export type InsertHaexBlockedDids = typeof haexBlockedDids.$inferInsert
export type SelectHaexBlockedDids = typeof haexBlockedDids.$inferSelect

// ---------------------------------------------------------------------------
// Invite Policy — controls who can send space invitations
// ---------------------------------------------------------------------------

export const haexInvitePolicy = sqliteTable(
  tableNames.haex.invite_policy.name,
  {
    id: text(tableNames.haex.invite_policy.columns.id).primaryKey(),
    policy: text(tableNames.haex.invite_policy.columns.policy).notNull().default('all'),
    updatedAt: text(tableNames.haex.invite_policy.columns.updatedAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
)
export type InsertHaexInvitePolicy = typeof haexInvitePolicy.$inferInsert
export type SelectHaexInvitePolicy = typeof haexInvitePolicy.$inferSelect

// ---------------------------------------------------------------------------
// Invite Outbox — CRDT-synced queue for delivering push invites
// Any device of the inviter can attempt delivery from this queue
// ---------------------------------------------------------------------------

export const haexInviteOutbox = sqliteTable(
  tableNames.haex.invite_outbox.name,
  {
    id: text(tableNames.haex.invite_outbox.columns.id).primaryKey(),
    spaceId: text(tableNames.haex.invite_outbox.columns.spaceId).notNull(),
    tokenId: text(tableNames.haex.invite_outbox.columns.tokenId).notNull(),
    targetDid: text(tableNames.haex.invite_outbox.columns.targetDid).notNull(),
    targetEndpointId: text(tableNames.haex.invite_outbox.columns.targetEndpointId).notNull(),
    status: text(tableNames.haex.invite_outbox.columns.status).notNull().default('pending'), // 'pending' | 'delivered' | 'expired' | 'failed'
    retryCount: integer(tableNames.haex.invite_outbox.columns.retryCount).notNull().default(0),
    nextRetryAt: text(tableNames.haex.invite_outbox.columns.nextRetryAt).default(sql`(CURRENT_TIMESTAMP)`),
    expiresAt: text(tableNames.haex.invite_outbox.columns.expiresAt).default(''),
    createdAt: text(tableNames.haex.invite_outbox.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
    /** Most recent delivery error message. Kept even after status becomes 'failed' so the UI can explain what went wrong. */
    lastError: text(tableNames.haex.invite_outbox.columns.lastError),
  },
)
export type InsertHaexInviteOutbox = typeof haexInviteOutbox.$inferInsert
export type SelectHaexInviteOutbox = typeof haexInviteOutbox.$inferSelect

// ---------------------------------------------------------------------------
// Invite Tokens — CRDT-synced so every device can validate ClaimInvite as leader
// ---------------------------------------------------------------------------

export const haexInviteTokens = sqliteTable(
  tableNames.haex.invite_tokens.name,
  {
    id: text(tableNames.haex.invite_tokens.columns.id).primaryKey(),
    spaceId: text(tableNames.haex.invite_tokens.columns.spaceId).notNull(),
    targetDid: text(tableNames.haex.invite_tokens.columns.targetDid),
    capabilities: text(tableNames.haex.invite_tokens.columns.capabilities), // JSON array
    preCreatedUcan: text(tableNames.haex.invite_tokens.columns.preCreatedUcan),
    includeHistory: integer(tableNames.haex.invite_tokens.columns.includeHistory, { mode: 'boolean' }).default(false),
    maxUses: integer(tableNames.haex.invite_tokens.columns.maxUses).notNull().default(1),
    currentUses: integer(tableNames.haex.invite_tokens.columns.currentUses).notNull().default(0),
    expiresAt: text(tableNames.haex.invite_tokens.columns.expiresAt).default(''),
    createdAt: text(tableNames.haex.invite_tokens.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
)
export type InsertHaexInviteTokens = typeof haexInviteTokens.$inferInsert
export type SelectHaexInviteTokens = typeof haexInviteTokens.$inferSelect
