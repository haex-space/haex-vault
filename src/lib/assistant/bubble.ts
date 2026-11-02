// The indicator of the voice assistant (spec 066 T047, T048, FR-041, FR-042): one look per phase,
// its position on the desktop, and the compact transcript. Pure — `scripts/check-assistant-ui.ts`
// tests it.
import type { AssistantPhase } from './wire.ts'

/** Device preference with the indicator's position (data-model.md, JSON `{x, y}`, 0–1). */
export const BUBBLE_POSITION_KEY = 'assistant.bubble.position'

export type BubblePosition = { x: number; y: number }

/** Bottom right, clear of a dock on the right or at the bottom. */
export const BUBBLE_DEFAULT: BubblePosition = { x: 0.92, y: 0.85 }

/** The Lucide icon of each phase, until the squirrel sprites (T118). */
export const PHASE_ICONS: Record<AssistantPhase, string> = {
  off: 'lucide:mic',
  ready: 'lucide:mic',
  listening: 'lucide:audio-lines',
  thinking: 'lucide:loader-circle',
  working: 'lucide:cog',
  speaking: 'lucide:volume-2',
  followUp: 'lucide:ear',
  awaitingAnswer: 'lucide:message-circle-question',
}

/** Phases whose icon turns while haex is busy. */
export const SPINNING_PHASES: readonly AssistantPhase[] = [
  'thinking',
  'working',
]

const clamp = (value: number) => Math.min(1, Math.max(0, value))

/** A stored position, or the default when it is missing or not `{x, y}` with numbers. */
export function parseBubblePosition(value: string | null): BubblePosition {
  if (value === null) return BUBBLE_DEFAULT
  try {
    const parsed: unknown = JSON.parse(value)
    if (parsed && typeof parsed === 'object') {
      const { x, y } = parsed as Record<string, unknown>
      if (
        typeof x === 'number' &&
        typeof y === 'number' &&
        Number.isFinite(x) &&
        Number.isFinite(y)
      ) {
        return { x: clamp(x), y: clamp(y) }
      }
    }
  } catch {
    // Not JSON: the default.
  }
  return BUBBLE_DEFAULT
}

/** The position of a point in an area of `width` × `height`, as a share of it (0–1). */
export function shareOf(
  point: { x: number; y: number },
  width: number,
  height: number,
): BubblePosition {
  return {
    x: width > 0 ? clamp(point.x / width) : BUBBLE_DEFAULT.x,
    y: height > 0 ? clamp(point.y / height) : BUBBLE_DEFAULT.y,
  }
}

/** An answer split at `spokenUntil` (characters): what haex said, and what it did not get to. */
export function splitSpoken(
  text: string,
  spokenUntil: number | null,
): { spoken: string; unspoken: string } {
  if (spokenUntil === null) return { spoken: text, unspoken: '' }
  const chars = [...text]
  const at = Math.min(Math.max(0, spokenUntil), chars.length)
  return {
    spoken: chars.slice(0, at).join(''),
    unspoken: chars.slice(at).join(''),
  }
}

/** How many entries the compact transcript shows: the last turns (FR-042). */
export const COMPACT_ENTRIES = 6

export function lastEntries<T>(entries: readonly T[]): T[] {
  return entries.slice(-COMPACT_ENTRIES)
}
