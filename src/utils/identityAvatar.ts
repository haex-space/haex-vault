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
  // Styles have distinct Options types; dispatching per-style keeps the
  // generic in `createAvatar<Options>` bound to one concrete shape instead
  // of an incompatible union.
  if (avatarOptions.style === 'toon-head') {
    return createAvatar(toonHead, avatarOptions as Parameters<typeof createAvatar<toonHead.Options>>[1]).toDataUri()
  }
  return createAvatar(bottts, avatarOptions as Parameters<typeof createAvatar<bottts.Options>>[1]).toDataUri()
}
