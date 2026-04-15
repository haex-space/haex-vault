import { and, eq } from 'drizzle-orm'
import { haexBlockedDids, haexIdentities, haexInvitePolicy } from '~/database/schemas'

type InvitePolicy = 'all' | 'contacts_only' | 'nobody'

const POLICY_ROW_ID = 'default'

export function useInvitePolicy() {
  const { currentVault } = storeToRefs(useVaultStore())

  const getDb = () => currentVault.value?.drizzle

  /**
   * Check if an invite from a given DID should be shown to the user.
   * Checks blocked DIDs and invite policy.
   */
  async function shouldShowInvite(inviterDid: string): Promise<boolean> {
    const db = getDb()
    if (!db) return false

    // 1. Check if DID is blocked
    const blocked = await db
      .select()
      .from(haexBlockedDids)
      .where(eq(haexBlockedDids.did, inviterDid))
      .limit(1)

    if (blocked.length > 0) return false

    // 2. Get invite policy
    const policy = await getPolicy()

    switch (policy) {
      case 'all':
        return true
      case 'nobody':
        return false
      case 'contacts_only': {
        // Check if the inviter's DID matches a known contact (identity without privateKey)
        const match = await db.select({ id: haexIdentities.id })
          .from(haexIdentities)
          .where(and(eq(haexIdentities.did, inviterDid), eq(haexIdentities.source, 'contact')))
          .limit(1)
        return match.length > 0
      }
      default:
        return true
    }
  }

  async function blockDid(did: string, label?: string): Promise<void> {
    const db = getDb()
    if (!db) throw new Error('No vault open')

    await db.insert(haexBlockedDids).values({
      id: crypto.randomUUID(),
      did,
      label: label ?? null,
      blockedAt: new Date().toISOString(),
    }).onConflictDoNothing()
  }

  async function unblockDid(did: string): Promise<void> {
    const db = getDb()
    if (!db) throw new Error('No vault open')

    await db.delete(haexBlockedDids).where(eq(haexBlockedDids.did, did))
  }

  async function setPolicy(policy: InvitePolicy): Promise<void> {
    const db = getDb()
    if (!db) throw new Error('No vault open')

    const existing = await db
      .select()
      .from(haexInvitePolicy)
      .where(eq(haexInvitePolicy.id, POLICY_ROW_ID))
      .limit(1)

    if (existing.length > 0) {
      await db.update(haexInvitePolicy).set({
        policy,
        updatedAt: new Date().toISOString(),
      }).where(eq(haexInvitePolicy.id, POLICY_ROW_ID))
    } else {
      await db.insert(haexInvitePolicy).values({
        id: POLICY_ROW_ID,
        policy,
        updatedAt: new Date().toISOString(),
      })
    }
  }

  async function getPolicy(): Promise<InvitePolicy> {
    const db = getDb()
    if (!db) return 'all'

    const rows = await db
      .select()
      .from(haexInvitePolicy)
      .where(eq(haexInvitePolicy.id, POLICY_ROW_ID))
      .limit(1)

    if (rows.length === 0) return 'all'
    return (rows[0]!.policy as InvitePolicy) ?? 'all'
  }

  async function getBlockedDids() {
    const db = getDb()
    if (!db) return []
    return db.select().from(haexBlockedDids)
  }

  return { shouldShowInvite, blockDid, unblockDid, setPolicy, getPolicy, getBlockedDids }
}
