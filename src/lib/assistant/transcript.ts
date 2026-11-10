/**
 * The transcript of a spoken conversation in the chat window (spec 066 T127, FR-047, FR-048):
 * entries as `assistant_transcript_list` and `assistant-transcript` deliver them, and the page of
 * `assistant_transcript_search`. Plain TypeScript so the check scripts can load it.
 */

/** A stored transcript entry (`TranscriptEntryWire`, contracts/tauri-commands.md). */
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

/** One hit of `assistant_transcript_search`. */
export type TranscriptHit = {
  threadId: string
  seq: number
  /** The entry itself: two entries of a thread can share a `seq` after a sync. */
  id: string
  snippet: string
}

/** A page of `assistant_transcript_search`. */
export type TranscriptSearchPage = {
  entries: TranscriptHit[]
  nextCursor?: string
}

/** Kinds that carry a small label; plain utterances and answers do not. */
export const LABELLED_KINDS: readonly string[] = [
  'announcement',
  'progress',
  'report',
  'approvalQuestion',
  'approvalAnswer',
  'choiceQuestion',
  'choiceAnswer',
  'stop',
  'closing',
  'notUnderstood',
  'secretNote',
]

/** Outcomes of approvals and choices that have a text. */
export const OUTCOMES: readonly string[] = [
  'approved',
  'declined',
  'open',
  'cancelled',
]

/** Speakers with a name; a value of a newer build (spec 067 participants) is shown as is. */
export const SPEAKERS: readonly string[] = ['user', 'haex']

/** The order of the table: `seq`, then `id` for equal values from two devices. */
export function compareEntries(a: TranscriptEntry, b: TranscriptEntry): number {
  if (a.seq !== b.seq) return a.seq - b.seq
  return a.id < b.id ? -1 : a.id > b.id ? 1 : 0
}

/** Adds or replaces `entry` (same id) and keeps the order of the table. */
export function mergeEntry(
  entries: readonly TranscriptEntry[],
  entry: TranscriptEntry,
): TranscriptEntry[] {
  return [...entries.filter((known) => known.id !== entry.id), entry].sort(
    compareEntries,
  )
}

/** Joins a freshly read list with the entries already shown, in one pass: the read wins for an id
 * it has, entries that arrived while reading stay. */
export function mergeStored(
  shown: readonly TranscriptEntry[],
  stored: readonly TranscriptEntry[],
): TranscriptEntry[] {
  const byId = new Map(shown.map((entry) => [entry.id, entry]))
  for (const entry of stored) byId.set(entry.id, entry)
  return [...byId.values()].sort(compareEntries)
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
