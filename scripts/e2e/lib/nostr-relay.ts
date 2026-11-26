// A Nostr relay for scenarios with more than one application process (spec 024, T078): the
// `e2e_nostr_relay` binary, the same in-process relay the integration tests use, built with
// `--features e2e` and run as a process the instances share, so devices find each other without a
// network. The relay never outlives its scenario.
import { existsSync, readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { targetDirectory } from './build.ts'
import { freePort } from './ports.ts'
import { spawnMarked, stopGroup } from './processes.ts'
import { reachFromDevice } from './platform/reach.ts'

const HERE = dirname(fileURLToPath(import.meta.url))
const REPO_ROOT = resolve(HERE, '..', '..', '..')
const URL_LINE = /stdout (ws:\/\/\S+)/

export interface NostrRelay {
  /** `ws://127.0.0.1:<port>`, what the instances are pointed at; the same after a stop and a start. */
  url: string
  /** `up` while the process runs. */
  readonly state: 'up' | 'down'
  /** Ends the process; safe to call twice. */
  stop(): Promise<void>
  /** Starts the process again on the same address; an error while it is up. */
  start(): Promise<void>
}

/** One running relay process. */
export interface RelayProcess {
  stop(): Promise<void>
}

/** Starts a relay process listening on `port`. */
export type LaunchRelay = (port: number) => Promise<RelayProcess>

/** The environment variable the relay binary reads its port from (src-tauri/src/bin/e2e_nostr_relay.rs). */
export const PORT_VARIABLE = 'HAEX_E2E_RELAY_PORT'

/** The state of a relay that can go down and come back on its address; the process comes from `launch`. */
export function createRelay(port: number, launch: LaunchRelay): NostrRelay {
  let running: RelayProcess | undefined
  return {
    url: `ws://127.0.0.1:${port}`,
    get state() {
      return running === undefined ? 'down' : 'up'
    },
    async start() {
      if (running !== undefined) throw new Error('the relay is already up')
      running = await launch(port)
    },
    async stop() {
      const process = running
      running = undefined
      if (process !== undefined) await process.stop()
    },
  }
}

/** The parts of a stored event a scenario looks at. */
export interface RelayEvent {
  id: string
  pubkey: string
  kind: number
  tags: string[][]
}

/**
 * The events the relay at `url` holds for `filter` (NIP-01 `REQ` until `EOSE`): what a scenario
 * checks when the devices' own state does not show that something left them.
 */
export async function relayEvents(
  url: string,
  filter: Record<string, unknown>,
  limitMs = 5_000,
): Promise<RelayEvent[]> {
  const socket = new WebSocket(url)
  const events: RelayEvent[] = []
  try {
    return await new Promise<RelayEvent[]>((done, fail) => {
      const timer = setTimeout(
        () =>
          fail(
            new Error(`the Nostr test relay did not answer in ${limitMs} ms`),
          ),
        limitMs,
      )
      socket.onerror = () => {
        clearTimeout(timer)
        fail(new Error(`the Nostr test relay at ${url} is not reachable`))
      }
      socket.onopen = () => socket.send(JSON.stringify(['REQ', 'q', filter]))
      socket.onmessage = (message) => {
        const [type, , payload] = JSON.parse(String(message.data)) as [
          string,
          string,
          RelayEvent,
        ]
        if (type === 'EVENT') events.push(payload)
        if (type === 'EOSE') {
          clearTimeout(timer)
          done(events)
        }
      }
    })
  } finally {
    socket.close()
  }
}

let built: Promise<string> | undefined

/** Builds the relay once per process and returns its path. */
export function buildNostrRelay(
  logFile: string,
  env: NodeJS.ProcessEnv = process.env,
): Promise<string> {
  built ??= (async () => {
    const binary = join(
      targetDirectory(REPO_ROOT, env),
      'debug',
      'e2e_nostr_relay',
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
        'e2e_nostr_relay',
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
        `building the Nostr test relay failed with exit status ${code}; see ${logFile}`,
      )
    }
    return binary
  })()
  return built
}

/** Extracts relay URLs from stdout log entries in order; a missing log yields an empty list. */
function printedUrls(logFile: string): string[] {
  const log = existsSync(logFile) ? readFileSync(logFile, 'utf8') : ''
  return [...log.matchAll(new RegExp(URL_LINE, 'g'))].map((m) => m[1] ?? '')
}

/**
 * Starts the relay on a port chosen here, so it can be stopped and started again on the same address,
 * and waits for the address it prints. The relay never outlives its scenario.
 * ponytail: the port is free when chosen, so another process could take it before the relay binds;
 * the relay would then end at once and the error says so. The upgrade path is a retry with a new port.
 */
export async function startNostrRelay(options: {
  logFile: string
  env?: NodeJS.ProcessEnv
  limitMs?: number
}): Promise<NostrRelay> {
  const env = options.env ?? process.env
  const binary = await buildNostrRelay(options.logFile, env)
  const launch: LaunchRelay = async (port) => {
    const before = printedUrls(options.logFile).length
    const { child } = spawnMarked(binary, [], {
      env: { ...env, [PORT_VARIABLE]: String(port) },
      logFile: options.logFile,
    })
    const stop = async () => {
      if (child.pid !== undefined) await stopGroup(child.pid)
    }
    const end = Date.now() + (options.limitMs ?? 15_000)
    for (;;) {
      if (child.exitCode !== null) {
        throw new Error(`the Nostr test relay ended; see ${options.logFile}`)
      }
      const printed = printedUrls(options.logFile)
      if (printed.length > before) {
        const url = printed[printed.length - 1]
        if (url !== `ws://127.0.0.1:${port}`) {
          await stop()
          throw new Error(
            `the Nostr test relay listens on ${url}, not on port ${port}; see ${options.logFile}`,
          )
        }
        return { stop }
      }
      if (Date.now() >= end) {
        await stop()
        throw new Error(
          `the Nostr test relay printed no address; see ${options.logFile}`,
        )
      }
      await new Promise((resolveWait) => setTimeout(resolveWait, 100))
    }
  }
  const port = await freePort()
  reachFromDevice(port)
  const relay = createRelay(port, launch)
  await relay.start()
  return relay
}
