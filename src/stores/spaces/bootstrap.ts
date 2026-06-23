import { eq } from 'drizzle-orm'
import type { SqliteRemoteDatabase } from 'drizzle-orm/sqlite-proxy'
import type { schema } from '~/database'
import { haexSpaces } from '~/database/schemas'
import type { SelectHaexSpaces } from '~/database/schemas'
import { SpaceType } from '~/database/constants'
import { createLogger } from '@/stores/logging'

type DB = SqliteRemoteDatabase<typeof schema>

const log = createLogger('SPACES:BOOTSTRAP')

export async function ensureVaultSpace(
  d: DB | undefined,
  vaultId: string,
  vaultName: string,
  ownerIdentityId: string | undefined,
) {
  if (!d) {
    log.error('ensureVaultSpace: no DB available')
    return
  }

  const existing = await d
    .select()
    .from(haexSpaces)
    .where(eq(haexSpaces.id, vaultId))
    .limit(1)
  if (existing.length > 0) {
    log.info(`Vault space ${vaultId} already exists`)
    return
  }

  if (!ownerIdentityId) {
    throw new Error('Cannot create vault space without an identity')
  }

  await d.insert(haexSpaces).values({
    id: vaultId,
    type: SpaceType.VAULT,
    name: vaultName,
    ownerIdentityId,
    originUrl: '',
  })
  log.info(`Created vault space "${vaultName}" (${vaultId})`)
}

export async function ensureDefaultSpace(
  d: DB | undefined,
  spaces: SelectHaexSpaces[],
  defaultName: string,
  fallbackOwnerIdentityId: string | undefined,
  reload: () => Promise<unknown>,
  createLocal: (name: string, ownerId: string) => Promise<unknown>,
) {
  if (!d) return

  const localSpaces = await d
    .select()
    .from(haexSpaces)
    .where(eq(haexSpaces.type, SpaceType.LOCAL))
    .limit(1)

  if (localSpaces.length > 0) {
    if (!spaces.find((s) => s.id === localSpaces[0]!.id)) {
      await reload()
    }
    return
  }

  const vaultOwnerId = spaces.find(
    (s) => s.type === SpaceType.VAULT,
  )?.ownerIdentityId
  const defaultOwnerId = vaultOwnerId || fallbackOwnerIdentityId
  if (!defaultOwnerId) {
    throw new Error('No identity available for default space')
  }
  await createLocal(defaultName, defaultOwnerId)
  log.info(`Default space "${defaultName}" created`)
}
