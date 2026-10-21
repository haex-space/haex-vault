// A stand-in for haex-pass-browser (spec 049, T035): the `e2e_bridge_client` binary, built with
// `--features e2e`, connects to the External Bridge of an instance like the extension and prints
// what it hears as `BRIDGE {json}` lines. A scenario starts it, waits for the events it expects and
// stops it; it never outlives its scenario.
import { existsSync, readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { targetDirectory } from './build.ts'
import { spawnMarked, stopGroup } from './processes.ts'

const HERE = dirname(fileURLToPath(import.meta.url))
const REPO_ROOT = resolve(HERE, '..', '..', '..')
const EVENT_LINE = /stdout BRIDGE (\{.*\})$/gm

/** One line the client printed: `handshake`, `authorization`, `reply`, `frame`, `closed`, `failed`. */
export type BridgeEvent = { event: string } & Record<string, unknown>

export interface BridgeClient {
  /** What this run printed so far. */
  events(): BridgeEvent[]
  /** Whether the process ended. */
  exited(): boolean
  /** Ends the process; safe to call twice. */
  stop(): Promise<void>
}

let built: Promise<string> | undefined

function buildBridgeClient(
  logFile: string,
  env: NodeJS.ProcessEnv,
): Promise<string> {
  built ??= (async () => {
    const binary = join(
      targetDirectory(REPO_ROOT, env),
      'debug',
      'e2e_bridge_client',
    )
    const { child, closed } = spawnMarked(
      'cargo',
      [
        'build',
        '--manifest-path',
        join(REPO_ROOT, 'src-tauri', 'Cargo.toml'),
        '--features',
        'e2e',
        '--bin',
        'e2e_bridge_client',
      ],
      { env: { ...env }, logFile, cwd: REPO_ROOT },
    )
    const code = await new Promise<number | null>((done) =>
      child.once('exit', (exitCode) => done(exitCode)),
    )
    await closed
    if (code !== 0 || !existsSync(binary)) {
      built = undefined
      throw new Error(
        `building the bridge test client failed with exit status ${code}; see ${logFile}`,
      )
    }
    return binary
  })()
  return built
}

/** The events a log holds, in order. */
export function printedEvents(log: string): BridgeEvent[] {
  return [...log.matchAll(EVENT_LINE)].map(
    (match) => JSON.parse(match[1] ?? '{}') as BridgeEvent,
  )
}

/** Starts a client against the bridge on `port`; `seed` picks its key, so the same seed is the
 * same client. */
export async function startBridgeClient(options: {
  port: number
  seed: number
  logFile: string
  hold?: boolean
  request?: { action: string; payload: Record<string, unknown> }
  env?: NodeJS.ProcessEnv
}): Promise<BridgeClient> {
  const env = options.env ?? process.env
  const binary = await buildBridgeClient(options.logFile, env)
  const readLog = () =>
    existsSync(options.logFile) ? readFileSync(options.logFile, 'utf8') : ''
  const before = printedEvents(readLog()).length
  const args = [String(options.port), String(options.seed)]
  if (options.hold) args.push('--hold')
  if (options.request) {
    args.push(
      '--request',
      options.request.action,
      JSON.stringify(options.request.payload),
    )
  }
  const { child } = spawnMarked(binary, args, {
    env: { ...env },
    logFile: options.logFile,
  })
  return {
    events: () => printedEvents(readLog()).slice(before),
    exited: () => child.exitCode !== null,
    async stop() {
      if (child.exitCode === null && child.pid !== undefined)
        await stopGroup(child.pid)
    },
  }
}
