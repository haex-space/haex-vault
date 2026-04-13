import type { SelectHaexIdentities } from '~/database/schemas'

/** Own identity with a guaranteed non-null private key. */
export type OwnIdentity = SelectHaexIdentities & { privateKey: string }

export class NoCurrentIdentityError extends Error {
  constructor() {
    super('No current identity available')
    this.name = 'NoCurrentIdentityError'
  }
}

/**
 * Resolves the current user's "own" identity (the first one with a private
 * key). Centralises the `loadIdentitiesAsync() + ownIdentities[0]` pattern
 * that previously appeared in several callsites.
 *
 * Throws `NoCurrentIdentityError` when no own identity exists — this is
 * treated as an exceptional state (the user cannot reach a vault-scoped view
 * without one), not a regular branch.
 */
export function useCurrentIdentity() {
  const identityStore = useIdentityStore()

  /**
   * Ensures the identity cache is warm and returns the first own identity.
   * Throws when none is available.
   */
  const ensureCurrentIdentityAsync = async (): Promise<OwnIdentity> => {
    await identityStore.loadIdentitiesAsync()
    const identity = identityStore.ownIdentities[0]
    if (!identity?.privateKey) throw new NoCurrentIdentityError()
    return identity as OwnIdentity
  }

  /**
   * Ensures the identity cache is warm and returns just the id.
   * Convenience wrapper for the common `identityId`-only callsites.
   */
  const ensureCurrentIdentityIdAsync = async (): Promise<string> => {
    const identity = await ensureCurrentIdentityAsync()
    return identity.id
  }

  return {
    ensureCurrentIdentityAsync,
    ensureCurrentIdentityIdAsync,
  }
}
