import { eq } from 'drizzle-orm'
import { isSpaceCapValue } from '@haex-space/ucan'
import { haexUcanTokens } from '~/database/schemas'
import type { SqliteRemoteDatabase } from 'drizzle-orm/sqlite-proxy'
import type { schema } from '~/database'

type DB = SqliteRemoteDatabase<typeof schema>

/** Get all capabilities the current user has for a given space.
 *
 * Task 8b: each row's `capabilities` column is a JSON `SpaceCapabilitySet`.
 * Callers still consume `space/<cap>` prefixed strings — the wire form the
 * frontend has always used — so this flattens the parsed sets back into
 * that shape (one string per `{cap, delegatable}` entry across all rows).
 */
export async function getCapabilitiesForSpace(db: DB, spaceId: string, myDids: string[]): Promise<string[]> {
  const tokens = await db.select()
    .from(haexUcanTokens)
    .where(eq(haexUcanTokens.spaceId, spaceId))

  const out = new Set<string>()
  for (const t of tokens) {
    if (!myDids.includes(t.audienceDid) && !myDids.includes(t.issuerDid)) continue
    let parsed: unknown
    try {
      parsed = JSON.parse(t.capabilities)
    } catch {
      continue
    }
    if (!isSpaceCapValue(parsed)) continue
    for (const entry of parsed) {
      out.add(`space/${entry.cap}`)
    }
  }
  return Array.from(out)
}

/** Check if the current user has a specific capability (or space/admin) for a space */
export async function hasCapability(db: DB, spaceId: string, capability: string, myDids: string[]): Promise<boolean> {
  const capabilities = await getCapabilitiesForSpace(db, spaceId, myDids)
  return capabilities.includes(capability) || capabilities.includes('space/admin')
}
