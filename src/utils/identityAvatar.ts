import { createAvatar } from '@dicebear/core'
import * as bottts from '@dicebear/bottts'
import * as toonHead from '@dicebear/toon-head'

/**
 * Generates a deterministic toon-head avatar (as a data URI) from a seed.
 * Use this when the user doesn't upload their own avatar during identity
 * creation — the public key is the conventional seed.
 */
export function generateToonHeadAvatar(seed: string): string {
  return createAvatar(toonHead, { seed }).toDataUri()
}

type AvatarStyle = 'bottts' | 'toon-head'

export function generateRandomAvatarOptions(
  style: AvatarStyle = 'toon-head',
): Record<string, unknown> {
  return {
    style,
    seed: crypto.randomUUID(),
  }
}

export function generateAvatarFromOptions(
  avatarOptions: Record<string, unknown>,
): string {
  const style = avatarOptions.style === 'toon-head' ? toonHead : bottts
  return createAvatar(style, avatarOptions).toDataUri()
}
