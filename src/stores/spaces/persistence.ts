import type { Ref } from 'vue'
import { eq, inArray } from 'drizzle-orm'
import type { SqliteRemoteDatabase } from 'drizzle-orm/sqlite-proxy'
import type { schema } from '~/database'
import { haexSpaces, haexSpaceMembers } from '~/database/schemas'
import type { SelectHaexSpaces } from '~/database/schemas'
import type { SpaceWithType } from './types'

type DB = SqliteRemoteDatabase<typeof schema>

export async function persistSpace(
  d: DB | undefined,
  space: SpaceWithType,
  reload: () => Promise<unknown>,
) {
  if (!d) return

  const existing = await d
    .select()
    .from(haexSpaces)
    .where(eq(haexSpaces.id, space.id))
    .limit(1)

  if (existing.length > 0) {
    await d
      .update(haexSpaces)
      .set({
        name: space.name,
        ownerIdentityId: space.ownerIdentityId,
        originUrl: space.originUrl || null,
        status: space.status,
        modifiedAt: new Date().toISOString(),
      })
      .where(eq(haexSpaces.id, space.id))
  } else {
    await d.insert(haexSpaces).values({
      id: space.id,
      type: space.type,
      name: space.name,
      ownerIdentityId: space.ownerIdentityId,
      originUrl: space.originUrl || null,
      status: space.status,
    })
  }

  await reload()
}

export async function removeSpaceFromDb(
  d: DB | undefined,
  spaces: Ref<SelectHaexSpaces[]>,
  spaceId: string,
) {
  if (d) {
    await d.delete(haexSpaces).where(eq(haexSpaces.id, spaceId))
  }
  spaces.value = spaces.value.filter((s) => s.id !== spaceId)
}

export async function loadMemberSpaceIds(
  d: DB | undefined,
  ownIdentityIds: string[],
  memberSpaceIds: Ref<Set<string>>,
) {
  if (!d) {
    memberSpaceIds.value = new Set()
    return
  }
  if (ownIdentityIds.length === 0) {
    memberSpaceIds.value = new Set()
    return
  }
  const rows = await d
    .select({ spaceId: haexSpaceMembers.spaceId })
    .from(haexSpaceMembers)
    .where(inArray(haexSpaceMembers.identityId, ownIdentityIds))
    .all()
  memberSpaceIds.value = new Set(rows.map((r) => r.spaceId))
}

export async function loadSpacesFromDb(
  d: DB | undefined,
  spaces: Ref<SelectHaexSpaces[]>,
  loadMembers: () => Promise<void>,
) {
  if (!d) return

  spaces.value = await d.select().from(haexSpaces)
  await loadMembers()

  return spaces.value
}
