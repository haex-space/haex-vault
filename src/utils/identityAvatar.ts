import { createAvatar } from '@dicebear/core'
import * as bottts from '@dicebear/bottts'
import * as toonHead from '@dicebear/toon-head'

export type AvatarStyle = 'bottts' | 'toon-head'

/**
 * Builds a minimal avatar-options record (style + seed). The seed drives
 * the deterministic pseudo-random sampling inside DiceBear — pass a known
 * value (e.g. an identity DID) when you want a stable avatar for existing
 * data, or omit it to get a fresh random UUID seed.
 *
 * The returned object is always persisted alongside `avatar` so that a
 * later re-render or a customizer open reproduces the exact same look.
 */
export function generateRandomAvatarOptions(
  style: AvatarStyle = 'toon-head',
  seed: string = crypto.randomUUID(),
): Record<string, unknown> {
  return { style, seed }
}

/**
 * Generates an avatar (data URI) + its options in one go. Used by every
 * identity/contact create path so new rows are never left with a null
 * `avatarOptions` — a missing options record is what causes the avatar
 * shown in the list to drift away from what the customizer previews,
 * because the customizer has nothing to initialize from.
 */
export function buildDefaultAvatarSet(
  style: AvatarStyle = 'toon-head',
  seed?: string,
): { avatar: string; avatarOptions: string } {
  const options = generateRandomAvatarOptions(style, seed)
  return {
    avatar: generateAvatarFromOptions(options),
    avatarOptions: JSON.stringify(options),
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
