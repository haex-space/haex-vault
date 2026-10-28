// Sync rules in the window (spec 061): defaults of a new rule, the interval in minutes, the
// state a rule shows, the hints of the dialog, and the paths of the folder picker. Pure, so
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
  return share === null ? null : Math.min(1, Math.max(0, share))
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
