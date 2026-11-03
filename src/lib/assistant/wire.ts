// The wire types of the voice assistant (spec 066, contracts/tauri-commands.md) and the pure
// helpers around them. Pure — `scripts/check-assistant-ui.ts` tests it.

/** `phase` of `AssistantStateWire` (contracts/tauri-commands.md). */
export type AssistantPhase =
  | 'off'
  | 'ready'
  | 'listening'
  | 'thinking'
  | 'working'
  | 'speaking'
  | 'followUp'
  | 'awaitingAnswer'

export type AssistantStateWire = {
  enabled: boolean
  muted: boolean
  paused: boolean
  phase: AssistantPhase
  threadId: string | null
  externalTts: boolean
  externalStt: boolean
  echoMode: 'full' | 'keywordOnly'
  openQuestion: {
    kind: 'approval' | 'choice' | 'screen'
    text: string
  } | null
}

export const ASSISTANT_OFF: AssistantStateWire = {
  enabled: false,
  muted: false,
  paused: false,
  phase: 'off',
  threadId: null,
  externalTts: false,
  externalStt: false,
  echoMode: 'keywordOnly',
  openQuestion: null,
}

/** The window's offset from UTC in minutes, as `assistant_set_enabled` takes it. */
export function utcOffsetMinutes(date = new Date()): number {
  return -date.getTimezoneOffset()
}
