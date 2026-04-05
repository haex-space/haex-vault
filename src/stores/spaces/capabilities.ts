import { eq } from 'drizzle-orm'
import { haexUcanTokens } from '~/database/schemas'
import type { SqliteRemoteDatabase } from 'drizzle-orm/sqlite-proxy'
import type { schema } from '~/database'

type DB = SqliteRemoteDatabase<typeof schema>

/** Get all capabilities the current user has for a given space */
export async function getCapabilitiesForSpace(db: DB, spaceId: string, myDids: string[]): Promise<string[]> {
  const tokens = await db.select()
    .from(haexUcanTokens)
    .where(eq(haexUcanTokens.spaceId, spaceId))

  return tokens
    .filter(t => myDids.includes(t.audienceDid) || myDids.includes(t.issuerDid))
    .map(t => t.capability)
}

/** Check if the current user has a specific capability (or space/admin) for a space */
export async function hasCapability(db: DB, spaceId: string, capability: string, myDids: string[]): Promise<boolean> {
  const capabilities = await getCapabilitiesForSpace(db, spaceId, myDids)
  return capabilities.includes(capability) || capabilities.includes('space/admin')
}
