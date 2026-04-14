import type { SelectHaexIdentities } from '~/database/schemas'
import {
  generateAvatarFromOptions,
  generateRandomAvatarOptions,
} from '~/utils/identityAvatar'

export interface CreateIdentityPayload {
  label: string
  avatar: string | null
  avatarOptions: Record<string, unknown> | null
  password: string
  claims: Array<{ type: string; value: string }>
}

/**
 * Orchestrates the full identity-creation flow:
 *   1. Creates the identity (new keypair) via the store.
 *   2. Saves the chosen avatar, or generates a random avatar configuration.
 *   3. Stashes the sync password (for the first backend registration).
 *   4. Batch-inserts any non-empty claims.
 *
 * Kept as a composable (not a store method) because the flow is UI-specific:
 * a headless creation would skip the avatar-fallback step.
 */
export function useIdentityCreation() {
  const identityStore = useIdentityStore()

  const createIdentityAsync = async (
    payload: CreateIdentityPayload,
  ): Promise<SelectHaexIdentities> => {
    const identity = await identityStore.createIdentityAsync(payload.label)

    // Avatar: use uploaded/customized image, or generate a random one.
    if (payload.avatar) {
      const optionsJson = payload.avatarOptions
        ? JSON.stringify(payload.avatarOptions)
        : null
      await identityStore.updateAvatarAsync(
        identity.id,
        payload.avatar,
        optionsJson,
      )
    } else {
      const avatarOptions = payload.avatarOptions ?? generateRandomAvatarOptions()
      await identityStore.updateAvatarAsync(
        identity.id,
        generateAvatarFromOptions(avatarOptions),
        JSON.stringify(avatarOptions),
      )
    }

    // Stash the sync password for first backend registration.
    if (payload.password) {
      identityStore.setIdentityPassword(identity.id, payload.password)
    }

    // Batch-insert claims, skipping empties.
    for (const claim of payload.claims) {
      const trimmed = claim.value.trim()
      if (!trimmed) continue
      await identityStore.addClaimAsync(identity.id, claim.type, trimmed)
    }

    return identity
  }

  return { createIdentityAsync }
}
