// Sync rules in the window (spec 061): defaults of a new rule, the interval in minutes, the
// state a rule shows, the hints of the dialog, the paths of the folder picker, the groups of a
// preview and the log depth. Pure, so
// `scripts/check-files-sync-rules.ts` checks it; no bindings with nested imports (typecheck:scripts).

export type SyncDirection = 'aToB' | 'bToA' | 'both'
export type SyncDeleteMode = 'mirror' | 'keep' | 'ask'
export type SyncState =
  'active' | 'running' | 'waiting' | 'error' | 'paused' | 'inactive'

/** The interval of a new rule in minutes (FR-005). */
export const DEFAULT_INTERVAL_MINUTES = 15
/** The shortest and the longest interval in minutes (data-model.md: 60 s to 86 400 s). */
export const MIN_INTERVAL_MINUTES = 1
export const MAX_INTERVAL_MINUTES = 1440

/** The delete mode a new rule starts with (FR-005): mirror both ways, keep one way. */
export function defaultDeleteMode(direction: SyncDirection): SyncDeleteMode {
  return direction === 'both' ? 'mirror' : 'keep'
}

/** Seconds of an interval typed in minutes, kept within the limits. */
export function intervalSecs(minutes: number): number {
  const whole = Number.isFinite(minutes)
    ? Math.round(minutes)
    : DEFAULT_INTERVAL_MINUTES
  const kept = Math.min(
    MAX_INTERVAL_MINUTES,
    Math.max(MIN_INTERVAL_MINUTES, whole),
  )
  return kept * 60
}

/** The state a rule shows: "holzi nicht aktiv" while the service does not run (FR-026). */
export function shownState(state: SyncState, serviceRuns: boolean): SyncState {
  return serviceRuns || state === 'paused' ? state : 'inactive'
}

/**
 * How far a running rule is, from 0 to 1, or `null` while it cannot tell (listing the places, or
 * nothing to copy): the bar then only shows that something runs.
 */
export function progressShare(progress: {
  phase: 'scanning' | 'copying'
  filesDone: number
  filesTotal: number
  bytesDone: number
  bytesTotal: number
}): number | null {
  if (progress.phase !== 'copying') return null
  const share =
    progress.bytesTotal > 0
      ? progress.bytesDone / progress.bytesTotal
      : progress.filesTotal > 0
        ? progress.filesDone / progress.filesTotal
        : null
  if (share === null) return null
  const clamped = Math.min(1, Math.max(0, share))
  return progress.filesDone < progress.filesTotal
    ? Math.min(0.99, clamped)
    : clamped
}

/** The places a rule copies into: its target, or both places of a two-way rule (FR-011). */
export function targetSides(direction: SyncDirection): ('a' | 'b')[] {
  if (direction === 'both') return ['a', 'b']
  return direction === 'bToA' ? ['a'] : ['b']
}

/** The columns of the rules list the user sorts by. */
export const RULE_SORT_KEYS = ['name', 'status', 'lastRun'] as const
export type RuleSortKey = (typeof RULE_SORT_KEYS)[number]
export type RuleSort = { key: RuleSortKey; ascending: boolean }

/** What the list needs of a rule to sort and filter it. */
export type RuleRow = {
  name: string
  state: SyncState
  /** ISO 8601, `null` before the first run. */
  lastRunAt: string | null
  /** Both places as the list shows them, for the filter. */
  places: string
}

/** Running and failing rules first when sorted by state, the idle ones last. */
const STATE_ORDER: Record<SyncState, number> = {
  running: 0,
  error: 1,
  waiting: 2,
  inactive: 3,
  active: 4,
  paused: 5,
}

/** The rows in the order of `sort`; ties keep the order by name. */
export function sortRules<T extends RuleRow>(
  rows: readonly T[],
  sort: RuleSort,
): T[] {
  const byName = (a: T, b: T) =>
    a.name.localeCompare(b.name, undefined, { numeric: true })
  const compare = (a: T, b: T): number => {
    if (sort.key === 'status')
      return STATE_ORDER[a.state] - STATE_ORDER[b.state]
    if (sort.key === 'lastRun') {
      // A rule that never ran counts as the oldest.
      const time = (row: T) => (row.lastRunAt ? Date.parse(row.lastRunAt) : 0)
      return time(a) - time(b)
    }
    return byName(a, b)
  }
  const sign = sort.ascending ? 1 : -1
  return [...rows].sort((a, b) => sign * compare(a, b) || byName(a, b))
}

/** The rows whose name or places contain every word of `text`, ignoring case. */
export function filterRules<T extends RuleRow>(
  rows: readonly T[],
  text: string,
): T[] {
  const words = text.toLowerCase().split(/\s+/).filter(Boolean)
  if (words.length === 0) return [...rows]
  return rows.filter((row) => {
    const haystack = `${row.name} ${row.places}`.toLowerCase()
    return words.every((word) => haystack.includes(word))
  })
}

/** The dialog asks for consent when files leave an encrypted folder for a plain one (FR-030a). */
export function needsDecryptConsent(
  aEncrypted: boolean,
  bEncrypted: boolean,
): boolean {
  return aEncrypted && !bEncrypted
}

/** The dialog points to encrypted folders when the target is a storage without (FR-031). */
export function showsCloudHint(
  bIsStorage: boolean,
  bEncrypted: boolean,
): boolean {
  return bIsStorage && !bEncrypted
}

/** The separator a device path uses. */
function separatorOf(path: string): string {
  return path.includes('\\') && !path.includes('/') ? '\\' : '/'
}

/** The folder above `path`; `null` at a root (`/`, `C:\`). */
export function parentPath(path: string): string | null {
  const sep = separatorOf(path)
  const trimmed =
    path.length > 1 && path.endsWith(sep) ? path.slice(0, -1) : path
  if (trimmed === sep || /^[A-Za-z]:\\?$/.test(trimmed)) return null
  const cut = trimmed.lastIndexOf(sep)
  if (cut < 0) return null
  if (cut === 0) return sep
  const parent = trimmed.slice(0, cut)
  return /^[A-Za-z]:$/.test(parent) ? `${parent}\\` : parent
}

/** `name` in the folder `path`. */
export function childPath(path: string, name: string): string {
  const sep = separatorOf(path)
  return path.endsWith(sep) ? `${path}${name}` : `${path}${sep}${name}`
}

/** The last name of a path, for labels; the path itself at a root. */
export function lastName(path: string): string {
  const sep = separatorOf(path)
  const trimmed =
    path.length > 1 && path.endsWith(sep) ? path.slice(0, -1) : path
  const cut = trimmed.lastIndexOf(sep)
  return cut < 0 || cut === trimmed.length - 1
    ? trimmed
    : trimmed.slice(cut + 1)
}

/** Whether the dialog may save: a name, two places, consent where needed. */
export function canSave(input: {
  name: string
  hasA: boolean
  hasB: boolean
  needsConsent: boolean
  consent: boolean
}): boolean {
  return (
    input.name.trim().length > 0 &&
    input.name.trim().length <= 80 &&
    input.hasA &&
    input.hasB &&
    (!input.needsConsent || input.consent)
  )
}

/** The actions of a preview in the order the window lists them. */
export const PLAN_ACTIONS = [
  'folder',
  'move',
  'conflictCopy',
  'copy',
  'adopt',
  'delete',
  'pending',
  'keep',
  'deviation',
] as const
export type PlanAction = (typeof PLAN_ACTIONS)[number]

/** The steps of a preview grouped by action, in the order of `PLAN_ACTIONS`; empty groups left out. */
export function planGroups<T extends { action: PlanAction }>(
  steps: readonly T[],
): { action: PlanAction; steps: T[] }[] {
  return PLAN_ACTIONS.map((action) => ({
    action,
    steps: steps.filter((step) => step.action === action),
  })).filter((group) => group.steps.length > 0)
}

/** The log depth of the setting (FR-044): a whole number from 0 to 100, or `null`. */
export function parseLogDepth(text: string): number | null {
  const trimmed = text.trim()
  if (!/^\d{1,3}$/.test(trimmed)) return null
  const depth = Number(trimmed)
  return depth <= 100 ? depth : null
}
