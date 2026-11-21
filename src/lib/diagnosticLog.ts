/**
 * The pure parts of the interface's side of the diagnostic log (spec 098, US2): what text a
 * console call becomes, which severities are sent, the limit on a flood and the level the plugin
 * has learned from the backend. No DOM global and no Tauri import, so a node test runs it.
 */

/** The longest text of one message, in characters; the backend cuts at 16 KiB on its side. */
export const MAX_TEXT = 8 * 1024
const CUT_MARK = '… [cut]'

/** The burst and the steady rate of messages the plugin sends (FR-014). */
const BURST = 200
const PER_SECOND = 100
const REPORT_EVERY_MS = 1000

export type UiLevel = 'error' | 'warn' | 'info' | 'debug'

/** The part of the backend's `DiagnosticLogStatus` the plugin uses (the binding imports a second
 * file, which the node checks cannot resolve). */
export interface LogStatus {
  level: 'standard' | 'verbose'
  sessionId: string | null
}

/** The text of one argument. An object is never serialised: its fields may hold an entry (FR-006). */
function argumentText(value: unknown): string {
  if (typeof value === 'string') return value
  if (value instanceof Error) return errorText(value)
  if (typeof value === 'function') return '[function]'
  if (Array.isArray(value)) return '[object]'
  if (typeof value === 'object' && value !== null) return '[object]'
  try {
    return String(value)
  } catch {
    return '[object]'
  }
}

/** An error's name, message and stack; WebKit's stack holds the frames only. */
function errorText(error: Error): string {
  const head = `${error.name}: ${error.message}`
  const stack = typeof error.stack === 'string' ? error.stack : ''
  return stack.includes(head) ? stack : stack ? `${head}\n${stack}` : head
}

/** The text of the arguments of a console call: joined with spaces, cut at `MAX_TEXT`. */
export function messageText(args: readonly unknown[]): string {
  const text = args.map(argumentText).join(' ')
  return text.length > MAX_TEXT ? text.slice(0, MAX_TEXT) + CUT_MARK : text
}

/** Whether a message of this severity is sent: info and debug only in "Ausführlich". */
export function allows(level: UiLevel, verbose: boolean): boolean {
  return verbose || level === 'error' || level === 'warn'
}

export interface Bucket {
  /** Takes one message's place; false when the flood limit is reached (the message is counted). */
  take(): boolean
  /** The messages dropped since the last report, at most once a second; 0 in between. */
  report(): number
}

/** A token bucket: a burst of 200, refilled at 100 a second. */
export function createBucket(now: () => number): Bucket {
  let tokens = BURST
  let refilled = now()
  let dropped = 0
  let reported = refilled

  const refill = () => {
    const at = now()
    tokens = Math.min(BURST, tokens + ((at - refilled) / 1000) * PER_SECOND)
    refilled = at
  }

  return {
    take() {
      refill()
      if (tokens >= 1) {
        tokens -= 1
        return true
      }
      dropped += 1
      return false
    },
    report() {
      const at = now()
      if (dropped === 0 || at - reported < REPORT_EVERY_MS) return 0
      const count = dropped
      dropped = 0
      reported = at
      return count
    },
  }
}

export interface LogState {
  current(): { verbose: boolean; sessionId: string | null }
  /** A change event of the backend: always applies. */
  onEvent(status: LogStatus): void
  /** The answer of the one read at the start: applies only if no event came first. */
  onStartup(status: LogStatus): void
}

/**
 * The level and the session token the plugin works with. It starts at "Standard" without a session
 * (the plugin loads before any unlock); an event of the backend is newer than the answer of the
 * read at the start, whenever that answer arrives (contracts/events.md).
 */
export function createLogState(): LogState {
  let verbose = false
  let sessionId: string | null = null
  let eventSeen = false
  return {
    current: () => ({ verbose, sessionId }),
    onEvent(status) {
      eventSeen = true
      verbose = status.level === 'verbose'
      sessionId = status.sessionId
    },
    onStartup(status) {
      if (eventSeen) return
      verbose = status.level === 'verbose'
      sessionId = status.sessionId
    },
  }
}
