import { eq, and } from 'drizzle-orm'
import { haexSpaceMembers, haexUcanTokens } from '~/database/schemas'
import type { SelectHaexSpaceMembers } from '~/database/schemas'
import type { SqliteRemoteDatabase } from 'drizzle-orm/sqlite-proxy'
import type { schema } from '~/database'
import { createLogger } from '@/stores/logging'

type DB = SqliteRemoteDatabase<typeof schema>

const log = createLogger('SPACES:MEMBERS')

/**
 * Add a member to a space. Derives and validates SPKI public key from DID.
 * Validates DID <-> public key match once at join time — after this, values are immutable.
 * Uses read-first pattern because CRDT partial unique indices are incompatible with ON CONFLICT.
 */
export async function addMemberToSpace(db: DB, params: {
  spaceId: string
  memberDid: string
  label: string
  role: string
  avatar?: string | null
  avatarOptions?: string | null
}) {
  const { didKeyToPublicKeyAsync } = await import('@haex-space/vault-sdk')
  const memberPublicKey = await didKeyToPublicKeyAsync(params.memberDid)

  const existing = await db.select({ id: haexSpaceMembers.id })
    .from(haexSpaceMembers)
    .where(and(eq(haexSpaceMembers.spaceId, params.spaceId), eq(haexSpaceMembers.memberDid, params.memberDid)))
    .limit(1)

  if (existing.length > 0) {
    await db.update(haexSpaceMembers)
      .set({
        label: params.label,
        role: params.role,
        avatar: params.avatar ?? null,
        avatarOptions: params.avatarOptions ?? null,
      })
      .where(and(eq(haexSpaceMembers.spaceId, params.spaceId), eq(haexSpaceMembers.memberDid, params.memberDid)))
  } else {
    await db.insert(haexSpaceMembers).values({
      spaceId: params.spaceId,
      memberDid: params.memberDid,
      memberPublicKey,
      label: params.label,
      role: params.role,
      avatar: params.avatar ?? null,
      avatarOptions: params.avatarOptions ?? null,
      joinedAt: new Date().toISOString(),
    })
  }
}

export async function getSpaceMembers(db: DB, spaceId: string): Promise<SelectHaexSpaceMembers[]> {
  return db.select().from(haexSpaceMembers).where(eq(haexSpaceMembers.spaceId, spaceId))
}

export async function updateOwnSpaceProfile(db: DB, myDids: string[], spaceId: string, profile: {
  label?: string
  avatar?: string | null
  avatarOptions?: string | null
}) {
  if (myDids.length === 0) return

  for (const did of myDids) {
    await db.update(haexSpaceMembers)
      .set(profile)
      .where(
        and(
          eq(haexSpaceMembers.spaceId, spaceId),
          eq(haexSpaceMembers.memberDid, did),
        ),
      )
  }
}

/** Lookup member public keys for signature verification. Returns Map<publicKey, memberDid> */
export async function getMemberPublicKeysForSpace(db: DB, spaceId: string): Promise<Map<string, string>> {
  const members = await db.select({
    memberPublicKey: haexSpaceMembers.memberPublicKey,
    memberDid: haexSpaceMembers.memberDid,
  }).from(haexSpaceMembers).where(eq(haexSpaceMembers.spaceId, spaceId))

  return new Map(members.map(m => [m.memberPublicKey, m.memberDid]))
}

export async function removeSpaceMember(db: DB, spaceId: string, memberDid: string) {
  await db.delete(haexSpaceMembers)
    .where(and(eq(haexSpaceMembers.spaceId, spaceId), eq(haexSpaceMembers.memberDid, memberDid)))

  log.info(`Removed member ${memberDid.slice(0, 20)}... from space ${spaceId}`)
}

/** One-time migration: populate haex_space_members from existing haex_ucan_tokens */
export async function migrateExistingMembers(
  db: DB,
  identities: Array<{ did: string; label: string; avatar: string | null; avatarOptions: string | null }>,
) {
  const allTokens = await db.select().from(haexUcanTokens)
  if (allTokens.length === 0) return

  const { didKeyToPublicKeyAsync } = await import('@haex-space/vault-sdk')

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
      const memberPublicKey = await didKeyToPublicKeyAsync(member.did)
      const knownIdentity = identities.find(i => i.did === member.did)

      await db.insert(haexSpaceMembers).values({
        spaceId: member.spaceId,
        memberDid: member.did,
        memberPublicKey,
        label: knownIdentity?.label || member.did.slice(8, 24),
        role: member.capability,
        avatar: knownIdentity?.avatar ?? null,
        avatarOptions: knownIdentity?.avatarOptions ?? null,
        joinedAt: new Date().toISOString(),
      }).onConflictDoNothing()
    } catch (error) {
      console.warn(`Failed to migrate member ${member.did}:`, error)
    }
  }
}
