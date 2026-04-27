import { eq, and, inArray } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import { didKeyToPublicKeyAsync } from '@haex-space/vault-sdk'
import {
  haexIdentities,
  haexSpaceMembers,
  haexUcanTokens,
} from '~/database/schemas'
import type {
  SelectHaexIdentities,
  SelectHaexSpaceMembers,
} from '~/database/schemas'
import type { SqliteRemoteDatabase } from 'drizzle-orm/sqlite-proxy'
import type { schema } from '~/database'
import { createLogger } from '@/stores/logging'

type DB = SqliteRemoteDatabase<typeof schema>

const log = createLogger('SPACES:MEMBERS')

export interface SpaceMemberWithIdentity {
  membership: SelectHaexSpaceMembers
  identity: SelectHaexIdentities
}

/**
 * Revoke UCAN tokens for the given DIDs in `spaceId`. Going through the
 * CRDT BEFORE-DELETE trigger, the deletes land in `haex_deleted_rows` and
 * propagate to peers on the next sync cycle. Single-purpose helper so all
 * "revoke member auth" call sites use the same predicate and propagation
 * path.
 */
export async function deleteUcansForMembersAsync(
  db: DB,
  spaceId: string,
  audienceDids: string[],
) {
  if (audienceDids.length === 0) return
  await db.delete(haexUcanTokens)
    .where(and(
      eq(haexUcanTokens.spaceId, spaceId),
      inArray(haexUcanTokens.audienceDid, audienceDids),
    ))
}

/**
 * Add a member to a space. Derives and validates SPKI public key from DID.
 * Validates DID <-> public key match once at join time — after this, values are immutable.
 * Uses read-first pattern because CRDT partial unique indices are incompatible with ON CONFLICT.
 */
export async function addMemberToSpace(db: DB, params: {
  spaceId: string
  identityId: string
  role: string
}) {
  const existing = await db.select({ id: haexSpaceMembers.id })
    .from(haexSpaceMembers)
    .where(and(eq(haexSpaceMembers.spaceId, params.spaceId), eq(haexSpaceMembers.identityId, params.identityId)))
    .limit(1)

  if (existing.length > 0) {
    await db.update(haexSpaceMembers)
      .set({
        role: params.role,
      })
      .where(and(eq(haexSpaceMembers.spaceId, params.spaceId), eq(haexSpaceMembers.identityId, params.identityId)))
  } else {
    await db.insert(haexSpaceMembers).values({
      spaceId: params.spaceId,
      identityId: params.identityId,
      role: params.role,
      joinedAt: new Date().toISOString(),
    })
  }
}

/**
 * Add the current user as a space member (non-fatal).
 * Centralises the repeated `loadIdentitiesAsync() + ownIdentities[0] + addMemberToSpace`
 * pattern that appeared in createLocalSpace, createOnlineSpace, claimInviteToken,
 * acceptLocalInvite, and acceptInviteAsync.
 */
export async function addSelfAsSpaceMember(
  db: DB,
  spaceId: string,
  identity: { id: string },
  role: string,
): Promise<void> {
  try {
    await addMemberToSpace(db, {
      spaceId,
      identityId: identity.id,
      role,
    })
  } catch (error) {
    log.warn(`Failed to add self as space member: ${error}`)
  }
}

export async function getSpaceMembers(db: DB, spaceId: string): Promise<SpaceMemberWithIdentity[]> {
  const rows = await db.select()
    .from(haexSpaceMembers)
    .innerJoin(haexIdentities, eq(haexSpaceMembers.identityId, haexIdentities.id))
    .where(eq(haexSpaceMembers.spaceId, spaceId))

  // innerJoin guarantees both sides are non-null at runtime, but Drizzle's
  // typings still widen to `| undefined` — assert non-null explicitly.
  return rows.map(row => ({
    membership: row.haex_space_members!,
    identity: row.haex_identities!,
  }))
}

export async function updateOwnSpaceProfile(db: DB, myIdentityIds: string[], _spaceId: string, profile: {
  name?: string
  avatar?: string | null
  avatarOptions?: string | null
}) {
  if (myIdentityIds.length === 0) return

  for (const identityId of myIdentityIds) {
    await db.update(haexIdentities)
      .set(profile)
      .where(
        eq(haexIdentities.id, identityId),
      )
  }
}

/** Lookup member public keys for signature verification. Returns Map<publicKey, memberDid> */
export async function getMemberPublicKeysForSpace(db: DB, spaceId: string): Promise<Map<string, string>> {
  const members = await db.select({
    did: haexIdentities.did,
  })
    .from(haexSpaceMembers)
    .innerJoin(haexIdentities, eq(haexSpaceMembers.identityId, haexIdentities.id))
    .where(eq(haexSpaceMembers.spaceId, spaceId))

  const pairs = await Promise.all(
    members.map(async (member) => [await didKeyToPublicKeyAsync(member.did), member.did] as const),
  )
  return new Map(pairs)
}

/**
 * Self-leave: hard-delete our own membership row(s) for `spaceId`.
 *
 * Membership DELETEs go through the CRDT BEFORE-DELETE trigger so the events
 * land in `haex_deleted_rows` and propagate to peers (incl. the leader) on
 * the next sync cycle. We do NOT touch the MLS group state — re-keying is
 * the leader's responsibility when they receive the propagated member-row
 * delete.
 *
 * UCAN tokens are kept by default and cleaned up by the FK-cascade when
 * the LEAVING space row is finally deleted (see
 * `cleanupCompletedLeavesAsync`). Without the UCAN, `PeerSession::connect`
 * cannot authenticate the LEAVING-state sync loop and the propagation
 * push would never run. Pass `deleteUcans: true` for the
 * online/synchronous leave path (remote DELETE acks the membership in
 * the same call, so the UCAN is no longer needed).
 *
 * Returns the membership count + the DIDs whose UCANs are pending deletion
 * (so callers — typically the cleanup pass — can finish what was started).
 */
export async function removeSelfFromSpace(
  db: DB,
  spaceId: string,
  ownIdentityIds: string[],
  options: { deleteUcans?: boolean } = {},
): Promise<{ removedMemberships: number; removedUcanDids: string[] }> {
  if (ownIdentityIds.length === 0) {
    log.warn(`removeSelfFromSpace: no own identities for space ${spaceId}`)
    return { removedMemberships: 0, removedUcanDids: [] }
  }

  const ownMemberships = await db.select({
    membershipId: haexSpaceMembers.id,
    did: haexIdentities.did,
  })
    .from(haexSpaceMembers)
    .innerJoin(haexIdentities, eq(haexSpaceMembers.identityId, haexIdentities.id))
    .where(and(
      eq(haexSpaceMembers.spaceId, spaceId),
      inArray(haexSpaceMembers.identityId, ownIdentityIds),
    ))

  if (ownMemberships.length === 0) {
    log.warn(`removeSelfFromSpace: no own membership found in space ${spaceId}`)
    return { removedMemberships: 0, removedUcanDids: [] }
  }

  const membershipIds = ownMemberships.map(m => m.membershipId)
  const ownDids = ownMemberships.map(m => m.did)

  if (options.deleteUcans) {
    await deleteUcansForMembersAsync(db, spaceId, ownDids)
  }

  await db.delete(haexSpaceMembers)
    .where(inArray(haexSpaceMembers.id, membershipIds))

  log.info(
    `Self-removed from space ${spaceId} (${membershipIds.length} memberships, ucans_revoked=${options.deleteUcans ?? false}, dids=${ownDids.length})`,
  )
  return { removedMemberships: membershipIds.length, removedUcanDids: ownDids }
}

export async function removeSpaceMember(db: DB, spaceId: string, memberDid: string) {
  const membership = await db.select({
    identityId: haexSpaceMembers.identityId,
  })
    .from(haexSpaceMembers)
    .innerJoin(haexIdentities, eq(haexSpaceMembers.identityId, haexIdentities.id))
    .where(and(eq(haexSpaceMembers.spaceId, spaceId), eq(haexIdentities.did, memberDid)))
    .limit(1)

  // 1. Find the member's leaf index in the MLS group
  const memberIndex = await invoke<number | null>('mls_find_member_index', { spaceId, memberDid })
  if (memberIndex === null) {
    log.warn(`Member ${memberDid.slice(0, 20)}... not found in MLS group, removing from DB only`)
    await deleteUcansForMembersAsync(db, spaceId, [memberDid])
    if (membership[0]) {
      await db.delete(haexSpaceMembers)
        .where(and(eq(haexSpaceMembers.spaceId, spaceId), eq(haexSpaceMembers.identityId, membership[0].identityId)))
    }
    return
  }

  // 2. MLS remove_member — creates a commit that rotates the group key
  const bundle = await invoke<{ commit: number[]; welcome: number[] | null; groupInfo: number[] }>(
    'mls_remove_member', { spaceId, memberIndex },
  )

  // 3. Broadcast commit to other members via local delivery
  if (bundle.commit.length > 0) {
    try {
      await invoke('local_delivery_broadcast_commit', { spaceId, commit: bundle.commit })
    }
    catch (error) {
      log.warn(`Failed to broadcast removal commit via local delivery: ${error}`)
      // Non-fatal: the commit is still valid, peers will get it on next sync
    }
  }

  // 4. Revoke UCAN tokens for the removed member (prevents further writes)
  await deleteUcansForMembersAsync(db, spaceId, [memberDid])

  // 5. Delete member from local DB (CRDT-synced to all devices)
  if (membership[0]) {
    await db.delete(haexSpaceMembers)
      .where(and(eq(haexSpaceMembers.spaceId, spaceId), eq(haexSpaceMembers.identityId, membership[0].identityId)))
  }

  // 6. Re-derive epoch key (forward secrecy — new key excludes removed member)
  await invoke('mls_export_epoch_key', { spaceId })

  log.info(`Removed member ${memberDid.slice(0, 20)}... from space ${spaceId} (MLS + UCAN revoked + DB)`)
}

/** One-time migration: populate haex_space_members from existing haex_ucan_tokens */
export async function migrateExistingMembers(
  db: DB,
  identities: Array<{ id: string; did: string }>,
) {
  const allTokens = await db.select().from(haexUcanTokens)
  if (allTokens.length === 0) return

  // Group by (spaceId, audienceDid) — pick highest capability
  const memberMap = new Map<string, { spaceId: string; did: string; capability: string }>()
  const roleOrder = ['admin', 'invite', 'write', 'read']

  for (const token of allTokens) {
    const key = `${token.spaceId}:${token.audienceDid}`
    const existing = memberMap.get(key)
    const tokenRole = token.capability.replace('space/', '')
    if (!existing || roleOrder.indexOf(tokenRole) < roleOrder.indexOf(existing.capability)) {
      memberMap.set(key, { spaceId: token.spaceId, did: token.audienceDid, capability: tokenRole })
    }
  }

  for (const member of memberMap.values()) {
    try {
      const knownIdentity = identities.find(i => i.did === member.did)
      if (!knownIdentity) continue

      await db.insert(haexSpaceMembers).values({
        spaceId: member.spaceId,
        identityId: knownIdentity.id,
        role: member.capability,
        joinedAt: new Date().toISOString(),
      }).onConflictDoNothing()
    } catch (error) {
      log.warn(`Failed to migrate member ${member.did}:`, error)
    }
  }
}
