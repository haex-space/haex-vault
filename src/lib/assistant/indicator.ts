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

/**
 * Whether dock and launcher show the toggle: with this device's preview setting on (T045), and
 * always while the assistant is on, so that it can be switched off after the preview was turned
 * off.
 */
export function toggleShown(preview: boolean, enabled: boolean): boolean {
  return preview || enabled
}
