// How the toggle of the voice assistant in the dock and the launcher shows its state (spec 066,
// FR-041, operator 2026-11-02: one toggle, no floating indicator). Pure —
// `scripts/check-assistant-ui.ts` tests it.
import type { AssistantPhase } from './wire.ts'

/** The Lucide icon of each phase, until the squirrel sprites (T118). */
export const PHASE_ICONS: Record<AssistantPhase, string> = {
  off: 'lucide:mic-off',
  ready: 'lucide:mic',
  listening: 'lucide:audio-lines',
  thinking: 'lucide:loader-circle',
  working: 'lucide:loader-circle',
  speaking: 'lucide:volume-2',
  followUp: 'lucide:audio-lines',
  awaitingAnswer: 'lucide:message-circle-question',
}

/** Phases whose icon turns while haex is busy. */
export const SPINNING_PHASES: readonly AssistantPhase[] = [
  'thinking',
  'working',
]

/** What switching on still needs (`Missing` of contracts/tauri-commands.md, FR-005). */
export type AssistantMissing =
  | 'sttModel'
  | 'vadModel'
  | 'wakeWordModel'
  | 'voice'
  | 'chatModel'
  | 'microphone'

/**
 * Where the toggle leads when something is missing (operator 2026-11-10: a click on a toggle that
 * is not set up yet opens the settings): the page of the first missing piece that has one, a path
 * of the settings app. A missing microphone has no page; `null` then.
 */
export function setupPath(missing: readonly AssistantMissing[]): string | null {
  for (const item of missing) {
    switch (item) {
      case 'sttModel':
        return '/ai/speech'
      case 'chatModel':
        return '/ai/local'
      case 'vadModel':
      case 'wakeWordModel':
      case 'voice':
        return '/ai/speech/assistant'
      case 'microphone':
        break
    }
  }
  return null
}

/** Whether the window switches the assistant on again after unlock (FR-002): it was on when the
 * vault was left, and it is not on yet. */
export function restoreWanted(
  stored: string | null,
  enabled: boolean,
): boolean {
  return stored === 'true' && !enabled
}
