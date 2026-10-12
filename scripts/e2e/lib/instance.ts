// One isolated instance of the application: its own directory tree, its own virtual screen, its own
// driver and ports, and a WebDriver session. Everything it starts carries the run's marker (FR-006,
// FR-008, FR-010).
import {
  chmodSync,
  existsSync,
  mkdirSync,
  readdirSync,
  realpathSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import net from 'node:net'
import { join } from 'node:path'
import type { ChildProcess } from 'node:child_process'
import { PortInUseError, freePort, withPortRetry } from './ports.ts'
import {
  MARKER_ENV,
  findByMarker,
  pidAlive,
  processGroupOf,
  spawnMarked,
  stopGroup,
} from './processes.ts'
import type { Tools } from './preflight.ts'
import { SessionGoneError, WebDriverClient } from './webdriver.ts'
import { createPage } from './page.ts'
import type { Page, StepRecorder } from './page.ts'

export type ColorScheme = 'light' | 'dark'

/**
 * What the application must not inherit from the maintainer's session: locations it would write to, the
 * desktop bus, and the display variables that would let it open a window on the real desktop.
 */
const NOT_INHERITED = new Set([
  'XDG_STATE_HOME',
  'XDG_SESSION_ID',
  'XDG_SESSION_TYPE',
  'XDG_SESSION_DESKTOP',
  'XDG_CURRENT_DESKTOP',
  'DISPLAY',
  'WAYLAND_DISPLAY',
  'XAUTHORITY',
  'SESSION_MANAGER',
  'GTK_THEME',
  'DBUS_SESSION_BUS_ADDRESS',
])

/**
 * The environment of the application and everything below it. Data, configuration, cache, runtime
 * directory and home are inside the instance root; the session bus is disabled (research R5: the
 * application reads the desktop's colour scheme through it, and a private bus starts host services that
 * outlive the run); GTK is pinned to X11 so a Wayland session can never receive the window.
 */
export function buildInstanceEnv(
  root: string,
  marker: string,
  parentEnv: NodeJS.ProcessEnv = process.env,
): Record<string, string> {
  const env: Record<string, string> = {}
  for (const [key, value] of Object.entries(parentEnv)) {
    if (value !== undefined && !NOT_INHERITED.has(key)) env[key] = value
  }
  return {
    ...env,
    XDG_DATA_HOME: join(root, 'data'),
    XDG_CONFIG_HOME: join(root, 'config'),
    XDG_CACHE_HOME: join(root, 'cache'),
    XDG_RUNTIME_DIR: join(root, 'run'),
    HOME: join(root, 'home'),
    DBUS_SESSION_BUS_ADDRESS: 'disabled:',
    GDK_BACKEND: 'x11',
    [MARKER_ENV]: marker,
  }
}

/**
 * Create the instance root: empty, except for the GTK settings file that fixes the colour scheme.
 * A root that already holds something is refused unless a fresh instance is deliberately reusing it.
 */
export function prepareRoot(
  root: string,
  options: { colorScheme?: ColorScheme; reuse?: boolean } = {},
): void {
  if (!options.reuse && existsSync(root) && readdirSync(root).length > 0) {
    throw new Error(`instance root ${root} is not empty`)
  }
  for (const dir of ['data', 'cache', 'home'])
    mkdirSync(join(root, dir), { recursive: true })
  mkdirSync(join(root, 'run'), { recursive: true, mode: 0o700 })
  chmodSync(join(root, 'run'), 0o700)
  mkdirSync(join(root, 'config', 'gtk-3.0'), { recursive: true })
  const dark = options.colorScheme === 'dark'
  writeFileSync(
    join(root, 'config', 'gtk-3.0', 'settings.ini'),
    `[Settings]\ngtk-application-prefer-dark-theme=${dark}\n`,
  )
}

export function removeRoot(root: string): void {
  rmSync(root, {
    recursive: true,
    force: true,
    maxRetries: 10,
    retryDelay: 100,
  })
}

/**
 * The command that starts the driver on its own virtual screen. The driver, the webview driver and the
 * application inherit the screen from `xvfb-run` and it ends with them.
 * ponytail: one X server per instance costs well under a second and keeps scenarios apart; the upgrade
 * path is one shared screen if start-up time ever dominates a large suite.
 *
 * `framebufferDir`, if given, keeps the screen's current XWD image at `<dir>/Xvfb_screen0` (research
 * R11, T068). `-fbdir` is an **Xvfb server** option, not an `xvfb-run` one: it must be appended inside
 * the same `-s` server-args string, not as a separate argument (`xvfb-run` would otherwise misparse it
 * as the command to run and never start the driver at all - found the hard way in T068's spike).
 */
export function driverCommand(
  tools: Tools,
  ports: { driver: number; native: number },
  options: { framebufferDir?: string } = {},
) {
  const screen =
    options.framebufferDir === undefined
      ? '-screen 0 1280x800x24'
      : `-screen 0 1280x800x24 -fbdir ${options.framebufferDir}`
  return {
    command: tools.xvfbRun,
    args: [
      '-a',
      '-s',
      screen,
      tools.tauriDriver,
      '--port',
      String(ports.driver),
      '--native-port',
      String(ports.native),
      '--native-driver',
      tools.webKitWebDriver,
    ],
  }
}

const sleep = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms))

export function portOpen(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = net.connect(port, '127.0.0.1')
    socket.once('connect', () => {
      socket.destroy()
      resolve(true)
    })
    socket.once('error', () => resolve(false))
  })
}

function hasEnded(child: ChildProcess): boolean {
  return (
    child.pid === undefined ||
    child.exitCode !== null ||
    child.signalCode !== null
  )
}

export function isPortInUseOutput(output: string): boolean {
  return /(?:eaddrinuse|(?:address|port).{0,40}(?:already in use|in use)|(?:already in use|in use).{0,40}(?:address|port)|unable to listen for http server.*\bport\b)/i.test(
    output,
  )
}

export interface StartInstanceOptions {
  /** The application under test, as given. */
  app: string
  tools: Tools
  /** The instance root. It is created here and removed by whoever owns the scenario. */
  root: string
  marker: string
  /** Driver, webview driver and application output goes here. */
  logFile: string
  colorScheme?: ColorScheme
  /** A fresh instance over the data of an earlier one. */
  reuse?: boolean
  /** How long the driver and the session may take to come up. */
  startLimitMs?: number
  parentEnv?: NodeJS.ProcessEnv
  /** Records a timeline entry on the scenario that started this instance. Default: does nothing. */
  step?: StepRecorder
  /** Keeps the screen's current XWD image at `<framebufferDir>/Xvfb_screen0` (research R11, T068). */
  framebufferDir?: string
}

/**
 * What only a phone has (spec 043, the Android scenarios): its system gestures, its screen and the
 * system's hand on the app's process. The platform layer provides it; a desktop instance has none.
 */
export interface PhoneControls {
  /** The system's back gesture. */
  back(): void
  /** Sends the app to the background and lets the system end its process, as under memory pressure. */
  killInBackground(): void
  /** Whether the system keeps the window out of screenshots and recordings. */
  screenProtected(): boolean
  /**
   * Sets the answer to the system's notification permission as if the person had given it: allowed,
   * or refused for good (the system then asks no more).
   */
  allowNotifications(allowed: boolean): void
  /** Refuses the microphone for good, as the person's second "Don't allow" does (FR-029). */
  refuseMicrophone(): void
  /** Gives "Zugriff auf alle Dateien" as the person does in the system's settings (spec 044 FR-039). */
  allowAllFiles(): void
  /** The package whose activity is in front, e.g. the system's settings. */
  frontPackage(): string | undefined
  /** The titles of the app's notifications the system shows now. */
  notificationTitles(): string[]
}

export interface Instance extends Page {
  root: string
  marker: string
  logFile: string
  ports: { driver: number; native: number }
  driverPid: number
  /** The pid of the application process this instance started. */
  pid: number
  screenshot(callLimitMs?: number): Promise<Buffer>
  alive(): boolean
  /** End the session and everything the instance started. Safe to call more than once. */
  stop(): Promise<void>
  /** Records a timeline entry on the scenario that started this instance (used by scripts/e2e/lib/flows.ts). */
  step: StepRecorder
  /** Present when the instance runs on a phone. */
  phone?: PhoneControls
}

async function launchDriver(
  options: StartInstanceOptions,
  env: Record<string, string>,
  limitMs: number,
) {
  return withPortRetry(async () => {
    const ports = { driver: await freePort(), native: await freePort() }
    const { command, args } = driverCommand(options.tools, ports, {
      framebufferDir: options.framebufferDir,
    })
    const logOffset = existsSync(options.logFile)
      ? statSync(options.logFile).size
      : 0
    const started = spawnMarked(command, args, {
      env,
      logFile: options.logFile,
    })
    const end = Date.now() + limitMs
    for (;;) {
      if (hasEnded(started.child)) {
        await started.closed
        const output = readFileSync(options.logFile)
          .subarray(logOffset)
          .toString()
        if (isPortInUseOutput(output)) {
          throw new PortInUseError(ports.driver)
        }
        throw new Error(
          `driver exited before listening on port ${ports.driver} (see ${options.logFile})\n${output.trim()}`,
        )
      }
      if (await portOpen(ports.driver)) return { ...started, ports }
      if (Date.now() >= end) {
        if (started.child.pid !== undefined) {
          await stopGroup(started.child.pid)
        }
        throw new Error(
          `tauri-driver did not listen on port ${ports.driver} within ${limitMs} ms (see ${options.logFile})`,
        )
      }
      await sleep(100)
    }
  })
}

/**
 * The driver's own port can answer (satisfying `launchDriver`'s poll) before its connection to the
 * native webview driver is ready behind it: seen for real, under load, as `POST /session got no answer`
 * - a `SessionGoneError`, even though the port itself was open a moment before. Repeated retries
 * cover it in practice (a scenario with several devices starts several drivers at once, and the native
 * backend can take tens of seconds to catch up under CI load); any other error, or a failure on the last
 * attempt, is not retried and reaches the caller as is.
 */
export const NEW_SESSION_ATTEMPTS = 16

export async function newSessionWithRetry(
  client: WebDriverClient,
  app: string | Record<string, unknown>,
): Promise<void> {
  for (let attempt = 1; ; attempt++) {
    try {
      await client.newSession(app)
      return
    } catch (error) {
      if (
        !(error instanceof SessionGoneError) ||
        !error.retryable ||
        attempt >= NEW_SESSION_ATTEMPTS
      ) {
        throw error
      }
      await sleep(300 * attempt)
    }
  }
}

/**
 * The application process of the instance whose driver leads process group `driverPid`. Every instance
 * of a run carries the same marker and runs the same executable, so with several instances up at once
 * only the group tells them apart.
 */
export function appInGroup(
  candidates: Array<{ pid: number }>,
  driverPid: number,
  groupOf: (pid: number) => number | null,
): number | undefined {
  return candidates.find(
    (p) => p.pid !== process.pid && groupOf(p.pid) === driverPid,
  )?.pid
}

export async function startInstance(
  options: StartInstanceOptions,
): Promise<Instance> {
  const limitMs = options.startLimitMs ?? 20_000
  const executable = realpathSync(options.app)
  prepareRoot(options.root, {
    colorScheme: options.colorScheme,
    reuse: options.reuse,
  })
  const env = buildInstanceEnv(options.root, options.marker, options.parentEnv)
  const { child, closed, ports } = await launchDriver(options, env, limitMs)
  const driverPid = child.pid
  if (driverPid === undefined) throw new Error('the driver process has no pid')
  const client = new WebDriverClient(`http://127.0.0.1:${ports.driver}`)

  const stop = async () => {
    try {
      await client.deleteSession()
    } catch {
      // The application may already have ended, which is what closing it does.
    }
    await stopGroup(driverPid)
    await Promise.race([closed, sleep(1000)])
  }

  let pid: number | undefined
  try {
    await newSessionWithRetry(client, options.app)
    // ponytail: a poll with a fixed deadline; the application appears within milliseconds of the session.
    const end = Date.now() + 5000
    while (pid === undefined && Date.now() < end) {
      pid = appInGroup(
        findByMarker(options.marker, executable),
        driverPid,
        processGroupOf,
      )
      if (pid === undefined) await sleep(50)
    }
    if (pid === undefined)
      throw new Error(
        'the application process did not appear after the session started',
      )
  } catch (error) {
    await stop()
    throw new Error(
      `could not start the application: ${(error as Error).message} (see ${options.logFile})`,
      {
        cause: error,
      },
    )
  }
  const appPid = pid
  const step: StepRecorder = options.step ?? (() => {})
  const page: Page = createPage({
    client,
    appPid,
    marker: options.marker,
    executable,
    step,
  })

  return {
    ...page,
    root: options.root,
    marker: options.marker,
    logFile: options.logFile,
    ports,
    driverPid,
    pid: appPid,
    screenshot: (callLimitMs) => client.screenshot(callLimitMs),
    alive: () => pidAlive(appPid),
    stop,
    step,
  }
}
