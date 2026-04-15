import { createAvatar } from '@dicebear/core'
import * as bottts from '@dicebear/bottts'
import * as toonHead from '@dicebear/toon-head'

export type AvatarStyle = 'bottts' | 'toon-head'

// ---------------------------------------------------------------------------
// Option types — canonical home. The customizer sub-components import
// these, and every store path that seeds a default avatar produces an
// object of the right shape so the customizer can re-hydrate it without
// falling back to generic defaults.
// ---------------------------------------------------------------------------

export interface ToonHeadOptions {
  style: 'toon-head'
  eyes: string
  eyebrows: string
  mouth: string
  beard: string
  beardProbability: number
  hair: string
  rearHair: string
  rearHairProbability: number
  clothes: string
  skinColor: string
  hairColor: string
  clothesColor: string
}

export interface BotttsOptions {
  style: 'bottts'
  face: string
  eyes: string
  mouth: string
  mouthProbability: number
  top: string
  topProbability: number
  sides: string
  sidesProbability: number
  texture: string
  textureProbability: number
  baseColor: string
}

export type AvatarOptions = ToonHeadOptions | BotttsOptions

// ---------------------------------------------------------------------------
// Randomizers
//
// Previously these lived inline in the customizer component and
// `buildDefaultAvatarSet` only emitted `{ style, seed }`. That caused a
// mismatch: UiAvatar rendered a seed-sampled SVG (e.g. a woman with curly
// hair), while the customizer merged the sparse options with its own
// hard-coded defaults and rendered a completely different avatar (the
// "default young man in blue shirt"). Emitting the *full* option set at
// create time keeps the list view and the customizer view in lockstep.
// ---------------------------------------------------------------------------

function pick<T>(arr: readonly T[]): T {
  return arr[Math.floor(Math.random() * arr.length)]!
}

export function randomToonHeadOptions(): ToonHeadOptions {
  return {
    style: 'toon-head',
    eyes: pick(['happy', 'wide', 'bow', 'humble', 'wink'] as const),
    eyebrows: pick(['raised', 'angry', 'happy', 'sad', 'neutral'] as const),
    mouth: pick(['laugh', 'angry', 'agape', 'smile', 'sad'] as const),
    beard: pick(['moustacheTwirl', 'fullBeard', 'chin', 'chinMoustache', 'longBeard'] as const),
    beardProbability: Math.random() < 0.5 ? 100 : 0,
    hair: pick(['sideComed', 'undercut', 'spiky', 'bun'] as const),
    rearHair: pick(['longStraight', 'longWavy', 'shoulderHigh', 'neckHigh'] as const),
    rearHairProbability: Math.random() < 0.5 ? 100 : 0,
    clothes: pick(['turtleNeck', 'openJacket', 'dress', 'shirt', 'tShirt'] as const),
    skinColor: pick(['f1c3a5', 'c68e7a', 'b98e6a', 'a36b4f', '5c3829'] as const),
    hairColor: pick(['2c1b18', 'a55728', 'b58143', 'd6b370', '724133', 'e8e1e1'] as const),
    clothesColor: pick(['545454', 'b11f1f', '0b3286', '147f3c', 'eab308', '731ac3', 'ec4899', 'f97316', '151613', 'e8e9e6'] as const),
  }
}

export function randomBotttsOptions(): BotttsOptions {
  return {
    style: 'bottts',
    face: pick(['round01', 'round02', 'square01', 'square02', 'square03', 'square04'] as const),
    eyes: pick(['bulging', 'dizzy', 'eva', 'frame1', 'frame2', 'glow', 'happy', 'hearts', 'robocop', 'round', 'roundFrame01', 'roundFrame02', 'sensor', 'shade01'] as const),
    mouth: pick(['bite', 'diagram', 'grill01', 'grill02', 'grill03', 'smile01', 'smile02', 'square01', 'square02'] as const),
    mouthProbability: Math.random() < 0.5 ? 100 : 0,
    top: pick(['antenna', 'antennaCrooked', 'bulb01', 'glowingBulb01', 'glowingBulb02', 'horns', 'lights', 'pyramid', 'radar'] as const),
    topProbability: Math.random() < 0.5 ? 100 : 0,
    sides: pick(['antenna01', 'antenna02', 'cables01', 'cables02', 'round', 'square', 'squareAssymetric'] as const),
    sidesProbability: Math.random() < 0.5 ? 100 : 0,
    texture: pick(['camo01', 'camo02', 'circuits', 'dirty01', 'dirty02', 'dots', 'grunge01', 'grunge02'] as const),
    textureProbability: Math.random() < 0.5 ? 100 : 0,
    baseColor: pick(['ffb300', '1e88e5', '546e7a', '6d4c41', '00acc1', 'f4511e', '5e35b1', '43a047', '757575', '3949ab', '039be5', '7cb342', 'c0ca33', 'fb8c00', 'd81b60', '8e24aa', 'e53935', '00897b', 'fdd835'] as const),
  }
}

export function defaultToonHeadOptions(): ToonHeadOptions {
  return {
    style: 'toon-head',
    eyes: 'happy',
    eyebrows: 'neutral',
    mouth: 'smile',
    beard: 'fullBeard',
    beardProbability: 0,
    hair: 'sideComed',
    rearHair: 'longStraight',
    rearHairProbability: 0,
    clothes: 'tShirt',
    skinColor: 'f1c3a5',
    hairColor: '2c1b18',
    clothesColor: '0b3286',
  }
}

export function defaultBotttsOptions(): BotttsOptions {
  return {
    style: 'bottts',
    face: 'round01',
    eyes: 'round',
    mouth: 'smile01',
    mouthProbability: 100,
    top: 'antenna',
    topProbability: 100,
    sides: 'round',
    sidesProbability: 100,
    texture: 'circuits',
    textureProbability: 0,
    baseColor: '1e88e5',
  }
}

// ---------------------------------------------------------------------------
// Rendering
//
// DiceBear expects array-wrapped values for option fields (and raw numbers
// for `*Probability` fields). The customizer already does this wrapping
// internally when rendering; doing it here too keeps the rendered SVG
// identical whether the caller is the list view (via this helper) or the
// customizer preview.
// ---------------------------------------------------------------------------

function toDiceBearOptions(options: AvatarOptions): Record<string, unknown> {
  const dice: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(options)) {
    if (key === 'style') continue
    dice[key] = typeof value === 'string' && !key.endsWith('Probability')
      ? [value]
      : value
  }
  return dice
}

function buildDiceBearAvatar(options: AvatarOptions) {
  const dice = toDiceBearOptions(options)
  if (options.style === 'toon-head') {
    return createAvatar(toonHead, dice as Parameters<typeof createAvatar<toonHead.Options>>[1])
  }
  return createAvatar(bottts, dice as Parameters<typeof createAvatar<bottts.Options>>[1])
}

/** SVG `data:` URI — what the DB persists as `avatar`. */
export function generateAvatarFromOptions(options: AvatarOptions): string {
  return buildDiceBearAvatar(options).toDataUri()
}

/** Raw SVG string — what the cropper / SVG-compressor pipelines expect. */
export function renderAvatarSvg(options: AvatarOptions): string {
  return buildDiceBearAvatar(options).toString()
}

/**
 * Builds a complete avatar + options pair for a new identity/contact.
 * Unlike the earlier `{ style, seed }` shape, this produces every field
 * the customizer expects so that re-opening the editor reproduces the
 * exact same avatar the user already saw in the list.
 */
export function buildDefaultAvatarSet(
  style: AvatarStyle = 'toon-head',
): { avatar: string; avatarOptions: string } {
  const options = style === 'toon-head' ? randomToonHeadOptions() : randomBotttsOptions()
  return {
    avatar: generateAvatarFromOptions(options),
    avatarOptions: JSON.stringify(options),
  }
}
