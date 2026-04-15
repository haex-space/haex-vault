import type { SelectHaexIdentities } from '~/database/schemas'

export interface CreateIdentityPayload {
  label: string
  avatar: string | null
  avatarOptions: Record<string, unknown> | null
  password: string
  claims: Array<{ type: string; value: string }>
}

/**
 * Orchestrates the full identity-creation flow:
 *   1. Creates the identity (new keypair) via the store — this also
 *      persists a default avatar + options so every row is immediately
 *      editable without a jump on first customizer open.
 *   2. If the user picked an avatar in the dialog (upload or customizer),
 *      override the store's default.
 *   3. Stashes the sync password (for the first backend registration).
 *   4. Batch-inserts any non-empty claims.
 */
export function useIdentityCreation() {
  const identityStore = useIdentityStore()

  const createIdentityAsync = async (
    payload: CreateIdentityPayload,
  ): Promise<SelectHaexIdentities> => {
    const identity = await identityStore.createIdentityAsync(payload.label)

    // Only override the auto-seeded avatar when the user explicitly picked
    // one in the dialog — otherwise we'd replace a perfectly good default
    // with a different random avatar (the original source of this bug).
    if (payload.avatar || payload.avatarOptions) {
      await identityStore.updateAvatarAsync(
        identity.id,
        payload.avatar,
        payload.avatarOptions ? JSON.stringify(payload.avatarOptions) : null,
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
