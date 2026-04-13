import { createAvatar } from '@dicebear/core'
import * as toonHead from '@dicebear/toon-head'

/**
 * Generates a deterministic toon-head avatar (as a data URI) from a seed.
 * Use this when the user doesn't upload their own avatar during identity
 * creation — the public key is the conventional seed.
 */
export function generateToonHeadAvatar(seed: string): string {
  return createAvatar(toonHead, { seed }).toDataUri()
}
