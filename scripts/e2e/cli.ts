// `pnpm test:e2e`: check the tools, remove leftovers of an earlier killed run, build the debug application
// (or take one by path), run every scenario in an isolated instance, and report (contracts/cli.md).
import { spawn } from 'node:child_process'
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  realpathSync,
} from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { parseArgs } from 'node:util'
import { fileURLToPath } from 'node:url'
import {
  BuildError,
  buildDebugApplication,
  resolveApplication,
  targetDirectory,
} from './lib/build.ts'
import type { ApplicationInfo } from './lib/build.ts'
import { buildNostrRelay } from './lib/nostr-relay.ts'
import { checkPreflight } from './lib/preflight.ts'
import type { ToolFacts, ToolName } from './lib/preflight.ts'
import { newMarker, stopGroup, stopRun, sweepOrphans } from './lib/processes.ts'
import type { MarkedProcess } from './lib/processes.ts'
import {
  buildReport,
  exitCodeFor,
  annotations,
  formatSummary,
  readResults,
  writeReport,
} from './lib/report.ts'
import type { RunFacts } from './lib/report.ts'
import type { CloseBehavior } from './lib/scenario.ts'
import { checkAndroidPreflight } from './lib/platform/android-preflight.ts'
import {
  EMULATOR_HOST,
  REDIRECT_ENV,
  redirectWorks,
} from './lib/platform/reach.ts'
import type { AndroidPreflight } from './lib/platform/android-preflight.ts'
import { PACKAGE } from './lib/platform/android.ts'
import { exclusionsFor } from './platform-exclusions.ts'

const HERE = dirname(fileURLToPath(import.meta.url))
const REPO_ROOT = resolve(HERE, '..', '..')
const SCENARIOS_DIR = join(HERE, 'scenarios')

export class UsageError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'UsageError'
  }
}

export interface CliOptions {
  app?: string
  closeBehavior?: CloseBehavior
  grep?: string
  /** Where the app runs: Linux processes (default) or an Android device (spec 043). */
  platform: 'linux' | 'android'
  /** Android: a debug APK to install before the run. */
  apk?: string
  /** Runs only this part of the scenarios, for parallel CI jobs. */
  shard?: { index: number; total: number }
  keep: boolean
  scenarioTimeoutSec: number
  runTimeoutSec: number
  timeScale: number
}

function positive(name: string, value: string): number {
  const number = Number(value)
  if (!Number.isFinite(number) || number <= 0)
    throw new UsageError(`${name} must be a positive number, got ${value}`)
  return number
}

export function parseCliOptions(
  argv: string[],
  env: NodeJS.ProcessEnv,
): CliOptions {
  let values: {
    app?: string
    'close-behavior'?: string
    grep?: string
    keep?: boolean
    'scenario-timeout'?: string
    'run-timeout'?: string
    platform?: string
    apk?: string
    shard?: string
  }
  try {
    values = parseArgs({
      args: argv,
      options: {
        app: { type: 'string' },
        'close-behavior': { type: 'string' },
        grep: { type: 'string' },
        keep: { type: 'boolean' },
        'scenario-timeout': { type: 'string' },
        'run-timeout': { type: 'string' },
        platform: { type: 'string' },
        apk: { type: 'string' },
        shard: { type: 'string' },
      },
      strict: true,
      allowPositionals: false,
    }).values
  } catch (error) {
    throw new UsageError((error as Error).message)
  }
  const closeBehavior = values['close-behavior']
  if (
    closeBehavior !== undefined &&
    closeBehavior !== 'exit' &&
    closeBehavior !== 'relaunch'
  ) {
    throw new UsageError(
      `--close-behavior must be exit or relaunch, got ${closeBehavior}`,
    )
  }
  const platform = values.platform ?? 'linux'
  if (platform !== 'linux' && platform !== 'android') {
    throw new UsageError(`--platform must be linux or android, got ${platform}`)
  }
  if (values.apk !== undefined && platform !== 'android') {
    throw new UsageError('--apk needs --platform android')
  }
  return {
    app: values.app ?? (env.E2E_APP === '' ? undefined : env.E2E_APP),
    closeBehavior,
    grep: values.grep,
    platform,
    apk: values.apk,
    shard: values.shard === undefined ? undefined : parseShard(values.shard),
    keep: values.keep === true,
    scenarioTimeoutSec: positive(
      '--scenario-timeout',
      values['scenario-timeout'] ?? '60',
    ),
    runTimeoutSec: positive('--run-timeout', values['run-timeout'] ?? '1800'),
    timeScale: positive('E2E_TIME_SCALE', env.E2E_TIME_SCALE ?? '1'),
  }
}

/** `i/n` with 1 ≤ i ≤ n. */
export function parseShard(value: string): { index: number; total: number } {
  const match = /^(\d+)\/(\d+)$/.exec(value)
  const index = Number(match?.[1])
  const total = Number(match?.[2])
  if (match === null || index < 1 || total < 1 || index > total) {
    throw new UsageError(`--shard must be i/n with 1 ≤ i ≤ n, got ${value}`)
  }
  return { index, total }
}

/** Every `total`-th scenario starting at `index`, so the shards split the run evenly. */
export function selectShard(
  names: string[],
  shard: { index: number; total: number } | undefined,
): string[] {
  if (shard === undefined) return names
  return names.filter(
    (_, position) => position % shard.total === shard.index - 1,
  )
}

/** The scenarios, by the base name of their files, in the order they run. */
export function listScenarioNames(dir: string = SCENARIOS_DIR): string[] {
  return readdirSync(dir)
    .filter((file) => file.endsWith('.test.ts'))
    .map((file) => file.slice(0, -'.test.ts'.length))
    .sort()
}

/** A scenario file must declare `scenario('<its own name>'`: the report and the result files key on it. */
export function findNameProblems(dir: string = SCENARIOS_DIR): string[] {
  const problems: string[] = []
  for (const name of listScenarioNames(dir)) {
    const source = readFileSync(join(dir, `${name}.test.ts`), 'utf8')
    if (!new RegExp(`scenario\\(\\s*'${name}'`).test(source)) {
      problems.push(`${name}.test.ts does not declare scenario('${name}'`)
    }
  }
  return problems
}

export interface PreflightOutcome {
  ok: boolean
  found: Partial<Record<ToolName, string>>
  tools: ToolFacts[]
  versions: { driver?: string; webview?: string }
  messages: string[]
}

export interface SignalSource {
  on(signal: NodeJS.Signals, handler: () => void): unknown
  off(signal: NodeJS.Signals, handler: () => void): unknown
}

export interface CliDeps {
  repoRoot: string
  /** The run directory is `<artifactsRoot>/<run id>`. */
  artifactsRoot: string
  preflight(): Promise<PreflightOutcome>
  /** The checks of an Android run; it installs `apk` first when given. */
  androidPreflight?(apk?: string): AndroidPreflight
  /** Whether the device reaches this machine through its network (`reach.ts`); a check replaces it. */
  redirectWorks?(serial: string): Promise<boolean>
  scenarioNames(): string[]
  scenarioNameProblems(): string[]
  newMarker(): string
  newRunId(): string
  sweepOrphans(): Promise<MarkedProcess[]>
  build(app: ApplicationInfo, runDir: string, marker: string): Promise<void>
  runScenarios(input: {
    names: string[]
    grep?: string
    runDir: string
    env: Record<string, string>
    signal: AbortSignal
  }): Promise<void>
  stopRun(marker: string): Promise<unknown>
  signals: SignalSource
  print(line: string): void
}

const PLACEHOLDER_TOOLS: ToolFacts[] = []

export async function runCli(
  argv: string[],
  env: NodeJS.ProcessEnv,
  deps: CliDeps,
): Promise<number> {
  let options: CliOptions
  try {
    options = parseCliOptions(argv, env)
  } catch (error) {
    deps.print(`${(error as Error).message}`)
    return 2
  }

  const marker = deps.newMarker()
  const runId = deps.newRunId()
  const runDir = join(deps.artifactsRoot, runId)
  mkdirSync(runDir, { recursive: true })
  const skippedHere =
    options.platform === 'android' ? exclusionsFor('android') : new Map()
  const names = selectShard(
    deps
      .scenarioNames()
      .filter(
        (name) => options.grep === undefined || name.includes(options.grep),
      )
      .filter((name) => !skippedHere.has(name)),
    options.shard,
  )

  const base = {
    runId,
    marker,
    tools: PLACEHOLDER_TOOLS,
    versions: {},
    timeScale: options.timeScale,
  }
  const finish = (
    facts: Partial<RunFacts> &
      Pick<RunFacts, 'outcome' | 'application' | 'expected'>,
  ): number => {
    const report = buildReport({ ...base, ...facts }, readResults(runDir))
    for (const line of formatSummary(report)) deps.print(line)
    if (env.GITHUB_ACTIONS === 'true') {
      for (const line of annotations(report)) deps.print(line)
    }
    writeReport(runDir, report)
    return exitCodeFor(report)
  }

  // 1. Preflight: nothing has been started yet.
  const problems = deps.scenarioNameProblems()
  if (problems.length > 0) {
    for (const problem of problems) deps.print(problem)
    return finish({
      outcome: 'preflight-failed',
      application: null,
      expected: [],
    })
  }
  let preflight: PreflightOutcome
  let platformEnv: Record<string, string> = {}
  let application: ApplicationInfo | null = null
  /** Android runs: the Linux build the other devices of a group run (stage 2: mixed groups). */
  let linuxApplication: ApplicationInfo | null = null
  let linuxFound: PreflightOutcome['found'] = {}
  const messages: string[] = []
  if (options.platform === 'android') {
    const android = (deps.androidPreflight ?? defaultAndroidPreflight(env))(
      options.apk,
    )
    preflight = {
      ok: android.ok,
      found: {},
      tools: PLACEHOLDER_TOOLS,
      versions: android.versions,
      messages: android.messages,
    }
    // The devices beside the phone reach it only through the run's iroh relay: the emulator's own
    // network loses the direct path under load (`sync::endpoint::app_bind_addr`, debug builds).
    platformEnv = {
      E2E_PLATFORM: 'android',
      HOLZI_E2E_SYNC_LOOPBACK: '1',
      ...android.env,
    }
    if (
      android.ok &&
      android.env[REDIRECT_ENV] === '1' &&
      !(await (deps.redirectWorks ?? redirectWorks)(
        android.env.ANDROID_SERIAL ?? '',
      ))
    ) {
      // Its network is not up: adb reverse does not need it (see `reach.ts`).
      platformEnv = Object.fromEntries(
        Object.entries(platformEnv).filter(([key]) => key !== REDIRECT_ENV),
      )
      deps.print(
        `the device does not reach this machine through its network (${EMULATOR_HOST}); using adb reverse`,
      )
    } else if (android.ok) {
      deps.print(
        platformEnv[REDIRECT_ENV] === '1'
          ? `the device reaches the run's services through its network (${EMULATOR_HOST})`
          : "the device reaches the run's services through adb reverse",
      )
    }
    // The app on the device; closing its vault always ends it (spec 043 FR-006).
    application = {
      path: PACKAGE,
      source: 'given',
      closeBehavior: 'exit',
      closeBehaviorFrom: 'flag',
    }
    for (const [name, entry] of skippedHere) {
      deps.print(
        `not on android: ${name} (${entry.kind === 'pending' ? `stage ${entry.stage}` : 'excluded'}: ${entry.reason})`,
      )
    }
    // The other devices of a group run on Linux, from the same debug build a Linux run makes. Without
    // the Linux tools here, stop before scenarios so the missing desktop setup is reported as a preflight failure.
    const linux = android.ok ? await deps.preflight() : undefined
    if (linux === undefined) {
      // The device check failed; the run stops below.
    } else if (linux.ok) {
      try {
        linuxApplication = resolveApplication({
          repoRoot: deps.repoRoot,
          env,
          appPath: options.app,
          closeBehaviorFlag: options.closeBehavior,
        })
        linuxFound = linux.found
      } catch (error) {
        messages.push(
          `the devices beside the phone cannot run: ${(error as Error).message}`,
        )
      }
    } else {
      messages.push(
        `the devices beside the phone cannot run here: ${linux.messages.join('; ')}`,
      )
    }
  } else {
    preflight = await deps.preflight()
    try {
      application = resolveApplication({
        repoRoot: deps.repoRoot,
        env,
        appPath: options.app,
        closeBehaviorFlag: options.closeBehavior,
      })
    } catch (error) {
      messages.push((error as Error).message)
    }
  }
  base.tools = preflight.tools
  base.versions = preflight.versions
  messages.unshift(...preflight.messages)
  if (!preflight.ok || application === null || messages.length > 0) {
    for (const message of messages) deps.print(message)
    return finish({ outcome: 'preflight-failed', application, expected: [] })
  }

  // The signal handlers stay until the end, so a second Ctrl-C during cleanup cannot leave leftovers.
  const controller = new AbortController()
  let interruptedBy: 'SIGINT' | 'SIGTERM' | undefined
  let runLimitReached = false
  const onInt = () => {
    interruptedBy ??= 'SIGINT'
    controller.abort()
  }
  const onTerm = () => {
    interruptedBy ??= 'SIGTERM'
    controller.abort()
  }
  deps.signals.on('SIGINT', onInt)
  deps.signals.on('SIGTERM', onTerm)
  try {
    // 2. Leftovers of a killed run.
    const swept = await deps.sweepOrphans()
    if (swept.length > 0) {
      deps.print(
        `removed ${swept.length} leftover process(es) of an earlier killed run (pid ${swept.map((p) => p.pid).join(', ')})`,
      )
    }

    // 3. Build.
    const toBuild = linuxApplication ?? application
    if (toBuild.source === 'built') {
      try {
        await deps.build(toBuild, runDir, marker)
      } catch (error) {
        deps.print((error as Error).message)
        await deps.stopRun(marker)
        return finish({ outcome: 'build-failed', application, expected: [] })
      }
    }

    // 4. Scenarios.
    const timer = setTimeout(() => {
      runLimitReached = true
      controller.abort()
    }, options.runTimeoutSec * 1000)
    try {
      await deps.runScenarios({
        names,
        grep: options.grep,
        runDir,
        signal: controller.signal,
        env: {
          E2E_RUN_DIR: runDir,
          E2E_APP: application.path,
          E2E_CLOSE_BEHAVIOR: application.closeBehavior,
          E2E_TOOLS: JSON.stringify(
            options.platform === 'android' ? linuxFound : preflight.found,
          ),
          ...(linuxApplication === null
            ? {}
            : { E2E_LINUX_APP: linuxApplication.path }),
          E2E_SCENARIO_TIMEOUT_MS: String(options.scenarioTimeoutSec * 1000),
          E2E_TIME_SCALE: String(options.timeScale),
          E2E_KEEP: options.keep ? '1' : '0',
          HOLZI_E2E_RUN: marker,
          ...platformEnv,
        },
      })
    } catch (error) {
      deps.print(`the scenario runner failed: ${(error as Error).message}`)
    } finally {
      clearTimeout(timer)
    }

    // 5. Sweep by this run's marker, then report.
    await deps.stopRun(marker)
    return finish({
      outcome: interruptedBy === undefined ? 'completed' : 'interrupted',
      interruptedBy,
      runLimitReached: runLimitReached && interruptedBy === undefined,
      application,
      expected: names,
    })
  } finally {
    deps.signals.off('SIGINT', onInt)
    deps.signals.off('SIGTERM', onTerm)
  }
}

function defaultAndroidPreflight(
  env: NodeJS.ProcessEnv,
): (apk?: string) => AndroidPreflight {
  return (apk) => checkAndroidPreflight({ env, apk })
}

function realDeps(env: NodeJS.ProcessEnv): CliDeps {
  return {
    repoRoot: REPO_ROOT,
    artifactsRoot:
      env.E2E_ARTIFACTS_DIR ?? join(targetDirectory(REPO_ROOT, env), 'e2e'),
    async preflight() {
      const check = await checkPreflight({ pathEnv: env.PATH })
      return {
        ok: check.result === 'ok',
        found: check.found,
        tools: check.tools,
        versions: {
          driver: check.driverVersion,
          webview: check.webviewVersion,
        },
        messages: check.messages,
      }
    },
    scenarioNames: () => listScenarioNames(),
    scenarioNameProblems: () => findNameProblems(),
    newMarker: () => newMarker(),
    newRunId() {
      const stamp = new Date()
        .toISOString()
        .replace(/[-:]/g, '')
        .replace(/\..*/, '')
        .replace('T', '-')
      return `${stamp}-${Math.random().toString(16).slice(2, 8)}`
    },
    sweepOrphans: () => sweepOrphans(),
    async build(app, runDir, marker) {
      await buildDebugApplication({
        repoRoot: REPO_ROOT,
        logFile: join(runDir, 'build.log'),
        marker,
      })
      if (!existsSync(app.path))
        throw new BuildError(
          `the build finished but ${app.path} does not exist`,
        )
    },
    async runScenarios({ names, runDir, env: scenarioEnv, signal }) {
      if (names.length === 0) return
      const files = names.map((name) => join(SCENARIOS_DIR, `${name}.test.ts`))
      // A scenario that shares a Nostr relay between instances needs the relay program (built with
      // `--features e2e`); it is built here, before any scenario's own deadline starts.
      if (
        files.some((file) => readFileSync(file, 'utf8').includes('nostrRelay('))
      ) {
        await buildNostrRelay(join(runDir, 'nostr-relay-build.log'), {
          ...process.env,
          ...scenarioEnv,
        })
      }
      // Own process group, so the terminal's Ctrl-C reaches only this command, which cleans up in order.
      const child = spawn(
        process.execPath,
        ['--test', '--test-concurrency=1', ...files],
        {
          cwd: REPO_ROOT,
          env: { ...process.env, ...scenarioEnv },
          stdio: 'inherit',
          detached: true,
        },
      )
      const exited = new Promise<void>((resolveExit) =>
        child.once('close', () => resolveExit()),
      )
      const onAbort = () => {
        if (child.pid !== undefined)
          void stopGroup(child.pid, { graceMs: 2000 })
      }
      if (signal.aborted) onAbort()
      signal.addEventListener('abort', onAbort, { once: true })
      await exited
      signal.removeEventListener('abort', onAbort)
    },
    stopRun: (marker) => stopRun(marker),
    signals: process,
    print: (line) => console.log(line),
  }
}

const invokedDirectly =
  process.argv[1] !== undefined &&
  realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)
if (invokedDirectly) {
  process.exitCode = await runCli(
    process.argv.slice(2),
    process.env,
    realDeps(process.env),
  )
}
