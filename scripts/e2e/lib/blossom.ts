// A Blossom-Server for scenarios that send chat files (spec 072, T105): the `e2e_blossom` binary,
// the in-memory server the unit tests use, built with `--features e2e` and run as a process the
// instances share. The server never outlives its scenario.
import { existsSync, readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { targetDirectory } from './build.ts'
import { freePort } from './ports.ts'
import { spawnMarked, stopGroup } from './processes.ts'
import { reachFromDevice } from './platform/reach.ts'

const HERE = dirname(fileURLToPath(import.meta.url))
const REPO_ROOT = resolve(HERE, '..', '..', '..')
const URL_LINE = /stdout (http:\/\/\S+)/

/** The environment variable the binary reads its port from (src-tauri/src/bin/e2e_blossom.rs). */
export const PORT_VARIABLE = 'HAEX_E2E_BLOSSOM_PORT'

export interface BlossomServer {
  /** `http://127.0.0.1:<port>/`, the address a vault takes as its Blossom-Server. */
  url: string
  /** Ends the process; safe to call twice. */
  stop(): Promise<void>
}

let built: Promise<string> | undefined

/** Builds the server once per process and returns its path. */
export function buildBlossomServer(
  logFile: string,
  env: NodeJS.ProcessEnv = process.env,
): Promise<string> {
  built ??= (async () => {
    const binary = join(targetDirectory(REPO_ROOT, env), 'debug', 'e2e_blossom')
    const { child, closed } = spawnMarked(
      'cargo',
      [
        'build',
        '--manifest-path',
        join(REPO_ROOT, 'src-tauri', 'Cargo.toml'),
        '--features',
        'e2e',
        '--bin',
        'e2e_blossom',
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
        `building the Blossom test server failed with exit status ${code}; see ${logFile}`,
      )
    }
    return binary
  })()
  return built
}

/** The addresses the server printed so far, in order; a missing log yields an empty list. */
function printedUrls(logFile: string): string[] {
  const log = existsSync(logFile) ? readFileSync(logFile, 'utf8') : ''
  return [...log.matchAll(new RegExp(URL_LINE, 'g'))].map((m) => m[1] ?? '')
}

/**
 * Starts the server on a port chosen here, so the address can be made reachable from a phone
 * (`reachFromDevice`) before any vault is pointed at it, and waits for the address it prints.
 * ponytail: the port is free when chosen, so another process could take it before the server binds;
 * the server would then end at once and the error says so. The upgrade path is a retry with a new
 * port, as for the Nostr relay.
 */
export async function startBlossomServer(options: {
  logFile: string
  env?: NodeJS.ProcessEnv
  limitMs?: number
}): Promise<BlossomServer> {
  const env = options.env ?? process.env
  const binary = await buildBlossomServer(options.logFile, env)
  const port = await freePort()
  reachFromDevice(port)
  const before = printedUrls(options.logFile).length
  const { child } = spawnMarked(binary, [], {
    env: { ...env, [PORT_VARIABLE]: String(port) },
    logFile: options.logFile,
  })
  const stop = async () => {
    if (child.pid !== undefined) await stopGroup(child.pid)
  }
  const expected = `http://127.0.0.1:${port}/`
  const end = Date.now() + (options.limitMs ?? 15_000)
  for (;;) {
    if (child.exitCode !== null) {
      throw new Error(`the Blossom test server ended; see ${options.logFile}`)
    }
    const printed = printedUrls(options.logFile)
    if (printed.length > before) {
      const url = printed[printed.length - 1]
      if (url !== expected) {
        await stop()
        throw new Error(
          `the Blossom test server listens on ${url}, not on port ${port}; see ${options.logFile}`,
        )
      }
      return { url: expected, stop }
    }
    if (Date.now() >= end) {
      await stop()
      throw new Error(
        `the Blossom test server printed no address; see ${options.logFile}`,
      )
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 100))
  }
}
