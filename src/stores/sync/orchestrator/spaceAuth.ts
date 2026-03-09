import { signSpaceChallengeAsync } from '@haex-space/vault-sdk'
import { orchestratorLog as log } from './types'

/**
 * Builds auth headers for sync requests.
 * - Personal backends: Bearer JWT from Supabase session
 * - Space backends: X-Space-Token + signed challenge (X-Space-Timestamp + X-Space-Signature)
 */
export const buildAuthHeadersAsync = async (
  spaceToken: string | null | undefined,
  spaceId: string | null | undefined,
  getAuthTokenAsync: () => Promise<string | null | undefined>,
): Promise<Record<string, string>> => {
  if (spaceToken) {
    const userKeypairStore = useUserKeypairStore()
    if (!userKeypairStore.privateKeyBase64) {
      throw new Error('User keypair not available for space challenge signing')
    }

    const challenge = await signSpaceChallengeAsync(spaceId!, userKeypairStore.privateKeyBase64)
    log.debug('Signed space challenge for', spaceId)

    return {
      'X-Space-Token': spaceToken,
      'X-Space-Timestamp': challenge.timestamp,
      'X-Space-Signature': challenge.signature,
    }
  }

  const token = await getAuthTokenAsync()
  if (!token) {
    throw new Error('Not authenticated')
  }
  return { Authorization: `Bearer ${token}` }
}
