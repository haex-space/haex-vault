// Every process the suite starts carries an environment marker, `HOLZI_E2E_RUN`, and the environment is
// inherited by everything below it, including WebKit's helper processes and a relaunched application.
// A run's processes are found by that marker alone, never by name, path or command line, so a
// maintainer's own Holzi (even the same binary) is never matched (FR-008).
import { spawn } from 'node:child_process'
import type { ChildProcess } from 'node:child_process'
import { randomBytes } from 'node:crypto'
import {
  createWriteStream,
  mkdirSync,
  readFileSync,
  readdirSync,
  readlinkSync,
} from 'node:fs'
import { dirname, join } from 'node:path'
import { createInterface } from 'node:readline'

export const MARKER_ENV = 'HOLZI_E2E_RUN'

export interface ParsedMarker {
  runnerPid: number
  runnerStart: string
  nonce: string
}

export interface MarkedProcess {
  pid: number
  marker: string
  exe: string | null
}

export interface StopOptions {
  /** How long a process gets to end after SIGTERM before it is killed. */
  graceMs?: number
}

const POLL_MS = 20

const sleep = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms))

/** Fields of `/proc/<pid>/stat` after the command name, starting with field 3 (the state). */
function statFields(pid: number): string[] | null {
  try {
    const stat = readFileSync(`/proc/${pid}/stat`, 'utf8')
    // The command name is in parentheses and may itself hold spaces and parentheses.
    return stat.slice(stat.lastIndexOf(')') + 2).split(' ')
  } catch {
    return null
  }
}

/** The start time of a process in clock ticks (field 22 of its stat), or null if it is gone. */
export function runnerStartTime(pid: number): string | null {
  return statFields(pid)?.[19] ?? null
}

/** Whether a process exists and is not a zombie waiting to be reaped. */
export function pidAlive(pid: number): boolean {
  try {
    process.kill(pid, 0)
  } catch (error) {
    return (error as NodeJS.ErrnoException).code === 'EPERM'
  }
  return statFields(pid)?.[0] !== 'Z'
}

/** The process group of a process, or null when it is gone. */
export function processGroupOf(pid: number): number | null {
  const group = Number(statFields(pid)?.[2])
  return Number.isInteger(group) ? group : null
}

/** `<runner pid>:<runner start time>:<random>`, identical for every process one run starts. */
export function newMarker(pid: number = process.pid): string {
  return `${pid}:${runnerStartTime(pid) ?? '0'}:${randomBytes(8).toString('hex')}`
}

export function parseMarker(value: string): ParsedMarker | null {
  const match = /^(\d+):(\d+):([0-9a-f]+)$/.exec(value)
  if (match === null) return null
  return { runnerPid: Number(match[1]), runnerStart: match[2], nonce: match[3] }
}

/** A marker's owner is alive only if its pid exists with the very same start time (pids get reused). */
function ownerAlive(marker: ParsedMarker): boolean {
  return runnerStartTime(marker.runnerPid) === marker.runnerStart
}

function readMarker(pid: number): string | null {
  try {
    const environ = readFileSync(`/proc/${pid}/environ`, 'latin1')
    for (const entry of environ.split('\0')) {
      if (entry.startsWith(`${MARKER_ENV}=`))
        return entry.slice(MARKER_ENV.length + 1)
    }
  } catch {
    // Not ours to read, or gone meanwhile.
  }
  return null
}

function readExe(pid: number): string | null {
  try {
    return readlinkSync(`/proc/${pid}/exe`)
  } catch {
    return null
  }
}

/**
 * Every process on the machine that carries some run's marker and that this user may inspect.
 * ponytail: reads the environment of each process in turn, so the cost grows linearly with the number of
 * processes (a few milliseconds on a desktop). Upgrade path if it ever hurts: watch the process
 * children of the run's own group instead of scanning everything.
 */
export function scanMarked(): MarkedProcess[] {
  const found: MarkedProcess[] = []
  for (const entry of readdirSync('/proc')) {
    if (!/^\d+$/.test(entry)) continue
    const pid = Number(entry)
    if (pid === process.pid) continue
    const marker = readMarker(pid)
    if (marker !== null) found.push({ pid, marker, exe: readExe(pid) })
  }
  return found
}

/** The processes of one run, optionally only those running the given executable. */
export function findByMarker(marker: string, exe?: string): MarkedProcess[] {
  return scanMarked().filter(
    (p) => p.marker === marker && (exe === undefined || p.exe === exe),
  )
}

/** Processes whose marker names a runner that no longer exists: leftovers of a killed run. */
export function findOrphans(): MarkedProcess[] {
  return scanMarked().filter((p) => {
    const parsed = parseMarker(p.marker)
    return parsed !== null && !ownerAlive(parsed)
  })
}

async function waitUntilGone(
  pids: number[],
  limitMs: number,
): Promise<boolean> {
  const end = Date.now() + limitMs
  while (Date.now() < end) {
    if (pids.every((pid) => !pidAlive(pid))) return true
    await sleep(POLL_MS)
  }
  return pids.every((pid) => !pidAlive(pid))
}

function signalAll(pids: number[], signal: NodeJS.Signals) {
  for (const pid of pids) {
    try {
      process.kill(pid, signal)
    } catch {
      // Already gone.
    }
  }
}

/** Ask the processes to end, then kill what is left after the grace period. */
async function terminate(pids: number[], graceMs: number) {
  signalAll(pids, 'SIGTERM')
  if (await waitUntilGone(pids, graceMs)) return
  signalAll(pids, 'SIGKILL')
  await waitUntilGone(pids, 1000)
}

/** Stop the leftovers of killed runs, and report what was stopped. */
export async function sweepOrphans(
  options: StopOptions = {},
): Promise<MarkedProcess[]> {
  const orphans = findOrphans()
  await terminate(
    orphans.map((p) => p.pid),
    options.graceMs ?? 1500,
  )
  return orphans
}

/** Stop every process that carries this run's marker, and report what was stopped. */
export async function stopRun(
  marker: string,
  options: StopOptions = {},
): Promise<MarkedProcess[]> {
  const found = findByMarker(marker)
  await terminate(
    found.map((p) => p.pid),
    options.graceMs ?? 1500,
  )
  return found
}

/**
 * Every process whose environment points at this instance folder: what the application started
 * and left behind, such as an app it opened through the system (`xdg-open`), which keeps running
 * after the application ended and keeps writing to the instance's cache. The other processes of the
 * run have their own folders, and this runner has none of them.
 */
export function findByRoot(root: string): MarkedProcess[] {
  const cache = `XDG_CACHE_HOME=${join(root, 'cache')}`
  const found: MarkedProcess[] = []
  for (const entry of readdirSync('/proc')) {
    if (!/^\d+$/.test(entry)) continue
    const pid = Number(entry)
    if (pid === process.pid) continue
    try {
      const environ = readFileSync(`/proc/${pid}/environ`, 'latin1').split('\0')
      if (environ.includes(cache)) {
        const marker = environ.find((e) => e.startsWith(`${MARKER_ENV}=`))
        found.push({
          pid,
          marker: marker?.slice(MARKER_ENV.length + 1) ?? '',
          exe: readExe(pid),
        })
      }
    } catch {
      // Not ours to read, or gone meanwhile.
    }
  }
  return found
}

/** Stop what an instance left running in its folder, and report what was stopped. */
export async function stopLeftovers(
  root: string,
  options: StopOptions = {},
): Promise<MarkedProcess[]> {
  const found = findByRoot(root)
  await terminate(
    found.map((p) => p.pid),
    options.graceMs ?? 1500,
  )
  return found
}

/** Stop a whole process group, for example everything one instance started. */
export async function stopGroup(
  pgid: number,
  options: StopOptions = {},
): Promise<void> {
  if (!Number.isInteger(pgid) || pgid <= 1) return
  const graceMs = options.graceMs ?? 1500
  const groupAlive = () => {
    try {
      process.kill(-pgid, 0)
      return true
    } catch {
      return false
    }
  }
  if (!groupAlive()) return
  try {
    process.kill(-pgid, 'SIGTERM')
  } catch {
    return
  }
  const end = Date.now() + graceMs
  while (Date.now() < end && groupAlive()) await sleep(POLL_MS)
  if (!groupAlive()) return
  try {
    process.kill(-pgid, 'SIGKILL')
  } catch {
    // Ended in the meantime.
  }
  const killEnd = Date.now() + 1000
  while (Date.now() < killEnd && groupAlive()) await sleep(POLL_MS)
}

export interface SpawnMarkedOptions {
  env: Record<string, string | undefined>
  /** Output goes here, each line with the time and its stream. */
  logFile: string
  cwd?: string
}

/**
 * Start a command in its own process group with its output written to a log file. `closed` resolves
 * when both output streams have ended and the log is flushed, or when the command could not start.
 */
export function spawnMarked(
  command: string,
  args: string[],
  options: SpawnMarkedOptions,
): { child: ChildProcess; closed: Promise<void> } {
  const child = spawn(command, args, {
    env: options.env,
    cwd: options.cwd,
    detached: true,
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  mkdirSync(dirname(options.logFile), { recursive: true })
  const log = createWriteStream(options.logFile, { flags: 'a' })
  log.on('error', () => {
    // A log that cannot be written must not take the run down with it.
  })
  const write = (stream: string, line: string) =>
    log.write(`${new Date().toISOString()} ${stream} ${line}\n`)
  const closed = new Promise<void>((resolve) => {
    let open = 2
    const finish = () => {
      open -= 1
      if (open === 0) log.end(() => resolve())
    }
    for (const [name, source] of [
      ['stdout', child.stdout],
      ['stderr', child.stderr],
    ] as const) {
      if (source === null) {
        finish()
        continue
      }
      const lines = createInterface({ input: source })
      lines.on('line', (line) => write(name, line))
      lines.on('close', finish)
    }
  })
  child.once('error', (error) =>
    write('stderr', `could not start ${command}: ${error.message}`),
  )
  return { child, closed }
}
