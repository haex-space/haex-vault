import type { DecryptedSpace } from '@haex-space/vault-sdk'
import type { SelectHaexSpaces } from '~/database/schemas'
import { SpaceType, SpaceStatus } from '~/database/constants'
import type {
  SpaceType as SpaceTypeValue,
  SpaceStatus as SpaceStatusValue,
} from '~/database/constants'

/** Extended space type including the DB type field (vault/online/local) */
export interface SpaceWithType extends DecryptedSpace {
  type: SpaceTypeValue
  status: SpaceStatusValue
  ownerIdentityId: string
}

export interface ResolvedIdentity {
  id: string
  publicKey: string
  privateKey: string
  did: string
  name: string
}

export const rowToSpace = (row: SelectHaexSpaces): SpaceWithType => ({
  id: row.id,
  name: row.name,
  type: (row.type as SpaceTypeValue) ?? SpaceType.ONLINE,
  status: (row.status as SpaceStatusValue) ?? SpaceStatus.ACTIVE,
  ownerIdentityId: row.ownerIdentityId,
  originUrl: row.originUrl ?? '',
  createdAt: row.createdAt ?? '',
  capabilities: [],
})
