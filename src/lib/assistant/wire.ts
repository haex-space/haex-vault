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

/** A stored transcript entry (`TranscriptEntryWire`, FR-047). */
export type TranscriptEntry = {
  id: string
  threadId: string
  seq: number
  spokenAt: number
  speaker: string
  kind: string
  text: string
  spokenUntil: number | null
  outcome: string | null
  refId: string | null
  createdAt: number
  final?: boolean
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

/** Entries in transcript order: `(seq, id)` as the backend reads them. */
export function sortEntries(entries: TranscriptEntry[]): TranscriptEntry[] {
  return [...entries].sort((a, b) =>
    a.seq !== b.seq ? a.seq - b.seq : a.id < b.id ? -1 : a.id > b.id ? 1 : 0,
  )
}

/** The window's offset from UTC in minutes, as `assistant_talk` takes it. */
export function utcOffsetMinutes(date = new Date()): number {
  return -date.getTimezoneOffset()
}
