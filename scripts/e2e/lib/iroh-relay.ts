// A local iroh relay for runs with a phone (spec 043, contract e2e-android.md): the phone sits behind
// the emulator's own network, so the devices on Linux cannot reach it directly. Every device of the
// group uses this relay instead; the phone reaches it through `adb reverse` at the same address. The
// program is the `iroh-relay` of the iroh release haex-vault uses, in its development mode (plain HTTP).
// The relay never outlives its scenario.
import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { freePort } from './ports.ts'
import { spawnMarked, stopGroup } from './processes.ts'
import { reachFromDevice } from './platform/reach.ts'

/** The environment variable that names the relay program; without it `iroh-relay` on the PATH. */
export const IROH_RELAY_VARIABLE = 'E2E_IROH_RELAY'

export interface IrohRelay {
  /** `http://127.0.0.1:<port>`, the same for every device and after a stop and a start. */
  url: string
  /** `up` while the process runs. */
  readonly state: 'up' | 'down'
  /** Ends the process; safe to call twice. */
  stop(): Promise<void>
  /** Starts the process again on the same address; an error while it is up. */
  start(): Promise<void>
}

/** The relay's settings: the port, and no metrics server (it would take a fixed port of its own). */
export function relayConfig(port: number): string {
  return `http_bind_addr = "127.0.0.1:${port}"\nenable_metrics = false\n`
}

/** Starts the relay and waits until it answers its health check. */
export async function startIrohRelay(options: {
  logFile: string
  env?: NodeJS.ProcessEnv
  limitMs?: number
}): Promise<IrohRelay> {
  const env = options.env ?? process.env
  const binary = env[IROH_RELAY_VARIABLE] || 'iroh-relay'
  const port = await freePort()
  const url = `http://127.0.0.1:${port}`
  // Beside the log, in the scenario's folder of the run.
  const config = join(dirname(options.logFile), 'iroh-relay.toml')
  mkdirSync(dirname(config), { recursive: true })
  writeFileSync(config, relayConfig(port))

  /** One process of the relay, up once it answers its health check. */
  const launch = async (): Promise<() => Promise<void>> => {
    const { child } = spawnMarked(binary, ['--dev', '--config-path', config], {
      env,
      logFile: options.logFile,
    })
    const end = async () => {
      if (child.pid !== undefined) await stopGroup(child.pid)
    }
    const until = Date.now() + (options.limitMs ?? 15_000)
    for (;;) {
      if (child.exitCode !== null) {
        throw new Error(`the iroh relay ended; see ${options.logFile}`)
      }
      try {
        if ((await fetch(`${url}/healthz`)).ok) return end
      } catch {
        // Not listening yet.
      }
      if (Date.now() >= until) {
        await end()
        throw new Error(
          `the iroh relay did not come up; see ${options.logFile}`,
        )
      }
      await new Promise((resolveWait) => setTimeout(resolveWait, 100))
    }
  }

  let running: (() => Promise<void>) | undefined = await launch()
  reachFromDevice(port, env)
  return {
    url,
    get state() {
      return running === undefined ? 'down' : 'up'
    },
    async start() {
      if (running !== undefined) throw new Error('the iroh relay is already up')
      running = await launch()
    },
    async stop() {
      const end = running
      running = undefined
      if (end !== undefined) await end()
    },
  }
}
