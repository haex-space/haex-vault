// The shell around a scenario: its context (an isolated instance on request, a timeline, waits), its
// deadline, its teardown and its result file. `runScenario` holds the logic and takes what it needs from
// outside; `scenario` registers it with Node's test runner.
import { test } from 'node:test'
import { randomBytes } from 'node:crypto'
import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { removeRoot, startInstance } from './instance.ts'
import type { Instance } from './instance.ts'
import { toTools } from './preflight.ts'
import { startNostrRelay } from './nostr-relay.ts'
import { startProvider } from './provider.ts'
import type { Provider } from './provider.ts'
import { captureFailure } from './artifacts.ts'
import { createGroup, maxDevicesFrom } from './group.ts'
import type { CaptureDevice } from './group.ts'
import { createLinuxHost } from './platform/linux.ts'
import { createAdb } from './platform/adb.ts'
import { createAndroidHost, startAndroidInstance } from './platform/android.ts'
import { startIrohRelay } from './iroh-relay.ts'
import type { IrohRelay } from './iroh-relay.ts'
import type { Tools } from './preflight.ts'
import { containerRuntimeAvailable } from './rustfs.ts'
import type {
  E2EEnv,
  RunDeps,
  ScenarioContext,
  ScenarioOptions,
  ScenarioResult,
  Step,
} from './scenario-types.ts'

export type {
  CloseBehavior,
  E2EEnv,
  FailureInfo,
  RunDeps,
  ScenarioContext,
  ScenarioOptions,
  ScenarioResult,
  ScenarioStatus,
  StartInstanceRequest,
  Step,
  WaitOptions,
} from './scenario-types.ts'

class WaitTimeoutError extends Error {
  description: string

  constructor(message: string, description: string) {
    super(message)
    this.description = description
  }
}
class ScenarioDeadlineError extends Error {}

const sleep = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms))

const names = new Set<string>()

/** Scenario names are unique because the name filter, the result file and the report all key on them. */
export function registerName(name: string): void {
  if (names.has(name)) throw new Error(`duplicate scenario name: ${name}`)
  names.add(name)
}

export function effectiveTimeoutMs(
  options: ScenarioOptions,
  env: E2EEnv,
): number {
  return Math.round(
    (options.timeoutMs ?? env.scenarioTimeoutMs) * env.timeScale,
  )
}

function skipReasonFor(
  options: ScenarioOptions,
  env: E2EEnv,
): string | undefined {
  if (options.needs?.phone && env.android === undefined)
    return 'runs only on a phone (spec 043)'
  if (options.needs?.container && !containerRuntimeAvailable())
    return 'no container runtime (docker) answers here'
  const needed = options.needs?.closeBehavior
  if (needed === undefined || needed === env.closeBehavior) return undefined
  return needed === 'relaunch'
    ? 'the build exits on close; this scenario needs one that relaunches'
    : 'the build relaunches on close; this scenario needs one that exits'
}

function describeObserved(value: unknown): string {
  try {
    return JSON.stringify(value) ?? String(value)
  } catch {
    return String(value)
  }
}

/** The last step reached before the failure, or the failing wait's own description (contracts/report.md). */
function failedStepFor(
  failure: unknown,
  steps: Step[],
  pendingStep?: string,
): string | undefined {
  if (failure instanceof WaitTimeoutError) return failure.description
  return steps.length > 0 ? steps[steps.length - 1]?.name : pendingStep
}

function writeResult(env: E2EEnv, result: ScenarioResult) {
  mkdirSync(join(env.runDir, 'results'), { recursive: true })
  writeFileSync(
    join(env.runDir, 'results', `${result.name}.json`),
    JSON.stringify(result, null, 2),
  )
}

export async function runScenario(
  name: string,
  options: ScenarioOptions,
  body: (ctx: ScenarioContext) => Promise<void>,
  deps: RunDeps,
): Promise<ScenarioResult> {
  const { env } = deps
  const startedAt = performance.now()
  const steps: Step[] = []
  const finish = (
    result: Omit<ScenarioResult, 'name' | 'durationMs' | 'steps'>,
  ): ScenarioResult => {
    const full: ScenarioResult = {
      name,
      durationMs: Math.round(performance.now() - startedAt),
      steps,
      ...result,
    }
    writeResult(env, full)
    return full
  }

  const skipReason = skipReasonFor(options, env)
  if (skipReason !== undefined) return finish({ status: 'skipped', skipReason })

  const controller = new AbortController()
  const teardowns: Array<() => Promise<void> | void> = []
  const instances: Instance[] = []
  const providers: Provider[] = []
  const deviceLists: Array<() => CaptureDevice[]> = []
  let pendingStep: string | undefined
  let instanceCount = 0

  const step = (stepName: string, detail?: string, device?: string) => {
    const entry: Step = {
      name: stepName,
      atMs: Math.round(performance.now() - startedAt),
      at: new Date().toISOString(),
    }
    if (detail !== undefined) entry.detail = detail
    if (device !== undefined) entry.device = device
    steps.push(entry)
  }
  const scaled = (ms: number) => Math.round(ms * env.timeScale)

  const ctx: ScenarioContext = {
    app: { path: env.app, closeBehavior: env.closeBehavior },
    name,
    signal: controller.signal,
    step,
    onTeardown: (action) => void teardowns.push(action),
    credentials: () => ({
      passphrase: randomBytes(18).toString('base64url'),
      providerKey: randomBytes(18).toString('base64url'),
    }),
    async waitFor(description, predicate, waitOptions = {}) {
      const limit =
        waitOptions.fixed === true
          ? (waitOptions.timeoutMs ?? 10_000)
          : scaled(waitOptions.timeoutMs ?? 10_000)
      // Never poll slower than every 50 ms: the promises under test are a second or a few.
      const interval = Math.min(waitOptions.intervalMs ?? 50, 50)
      const end = performance.now() + limit
      for (;;) {
        if (controller.signal.aborted) {
          throw new WaitTimeoutError(
            `stopped while waiting for ${description}`,
            description,
          )
        }
        const observed = await predicate()
        if (observed) return observed
        if (performance.now() >= end) {
          throw new WaitTimeoutError(
            `timed out after ${limit} ms waiting for ${description} (last observed: ${describeObserved(observed)})`,
            description,
          )
        }
        await sleep(interval)
      }
    },
    async startInstance(instanceOptions = {}) {
      instanceCount += 1
      const root =
        instanceOptions.reusesRoot ??
        join(env.runDir, 'instances', `${name}-${instanceCount}`)
      pendingStep = 'instance-ready'
      try {
        const instance = await deps.startInstance({
          scenario: name,
          root,
          logFile: join(env.runDir, name, 'driver.log'),
          colorScheme: instanceOptions.colorScheme,
          reuse: instanceOptions.reusesRoot !== undefined,
          env,
          step,
          framebufferDir: instanceOptions.framebufferDir,
          phoneScreen: instanceOptions.phoneScreen,
          withoutAllFiles: instanceOptions.withoutAllFiles,
        })
        instances.push(instance)
        // The root is removed with the scenario, not when the instance stops, so a fresh instance can reuse it.
        teardowns.push(async () => {
          await instance.stop()
          removeRoot(root)
        })
        step('instance-ready')
        return instance
      } catch (error) {
        removeRoot(root)
        throw error
      } finally {
        pendingStep = undefined
      }
    },
    async nostrRelay() {
      const relay = await startNostrRelay({
        logFile: join(env.runDir, name, 'nostr-relay.log'),
      })
      teardowns.push(() => relay.stop())
      return relay
    },
    async group(spec) {
      if (deps.createHost === undefined) {
        throw new Error(
          'this run has no driver layer, so it cannot make a group',
        )
      }
      // A phone sits behind the emulator's network: the devices reach each other through a local
      // iroh relay (spec 043).
      const phoneHost = deps.createPhoneHost?.({ scenario: name, env })
      let irohRelay: IrohRelay | undefined
      if (phoneHost !== undefined) {
        const started = await startIrohRelay({
          logFile: join(env.runDir, name, 'iroh-relay.log'),
        })
        teardowns.push(() => started.stop())
        irohRelay = started
      }
      return createGroup(
        {
          host: deps.createHost({ scenario: name, env }),
          phoneHost,
          irohRelay,
          irohRelays: irohRelay === undefined ? undefined : [irohRelay.url],
          relay: await ctx.nostrRelay(),
          credentials: ctx.credentials,
          onTeardown: ctx.onTeardown,
          keep: env.keep,
          maxDevices: maxDevicesFrom(process.env),
          waitFor: ctx.waitFor,
          step,
          captureWith: (devices) => void deviceLists.push(devices),
        },
        spec,
      )
    },
    async provider(behavior, options) {
      const started = await startProvider(behavior, options)
      providers.push(started)
      teardowns.push(() => started.close())
      return started
    },
  }

  const limit = effectiveTimeoutMs(options, env)
  let failure: unknown
  let timer: NodeJS.Timeout | undefined
  const deadline = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      controller.abort()
      reject(
        new ScenarioDeadlineError(
          `scenario ${name} reached its deadline of ${limit} ms`,
        ),
      )
    }, limit)
  })
  try {
    await Promise.race([body(ctx), deadline])
  } catch (error) {
    failure = error
  } finally {
    clearTimeout(timer)
  }

  if (failure !== undefined && deps.onFailure !== undefined) {
    const failedStep = failedStepFor(failure, steps, pendingStep)
    try {
      await deps.onFailure({
        scenario: name,
        env,
        error: failure,
        steps,
        instances,
        providers,
        devices: deviceLists.flatMap((list) => list()),
        failedStep,
        deadlineMs:
          failure instanceof ScenarioDeadlineError ? limit : undefined,
      })
    } catch {
      // Diagnostics must never hide the failure they describe.
    }
  }

  const teardownErrors: string[] = []
  for (const action of teardowns.reverse()) {
    try {
      await action()
    } catch (error) {
      teardownErrors.push((error as Error).message)
    }
  }

  if (failure !== undefined) {
    const failedStep = failedStepFor(failure, steps, pendingStep)
    const message = failure instanceof Error ? failure.message : String(failure)
    return finish({
      status: 'failed',
      // A teardown that failed too is part of the picture (it can leave the device to the next one).
      error:
        teardownErrors.length > 0
          ? `${message}\nteardown failed: ${teardownErrors.join('; ')}`
          : message,
      failedStep,
      material: join(env.runDir, name),
    })
  }
  if (teardownErrors.length > 0) {
    const teardownFailure = new Error(
      `teardown failed: ${teardownErrors.join('; ')}`,
    )
    const failedStep = failedStepFor(teardownFailure, steps)
    if (deps.onFailure !== undefined) {
      try {
        await deps.onFailure({
          scenario: name,
          env,
          error: teardownFailure,
          steps,
          instances,
          providers,
          devices: deviceLists.flatMap((list) => list()),
          failedStep,
        })
      } catch {
        // Diagnostics must never hide the failure they describe.
      }
    }
    return finish({
      status: 'failed',
      error: teardownFailure.message,
      failedStep,
      material: join(env.runDir, name),
    })
  }
  // Nothing to show for a pass: the material (the instance's own continuous driver.log, and anything a
  // failure would have added) is removed unless the maintainer asked to keep it too (contracts/report.md).
  if (!env.keep)
    rmSync(join(env.runDir, name), { recursive: true, force: true })
  return finish({ status: 'passed' })
}

/** Read what the command passes to the scenario files. */
export function readEnv(source: NodeJS.ProcessEnv = process.env): E2EEnv {
  const need = (key: string): string => {
    const value = source[key]
    if (value === undefined || value === '') {
      throw new Error(
        `${key} is not set: scenarios are started by "pnpm test:e2e", not run on their own`,
      )
    }
    return value
  }
  const closeBehavior = need('E2E_CLOSE_BEHAVIOR')
  if (closeBehavior !== 'exit' && closeBehavior !== 'relaunch') {
    throw new Error(
      `E2E_CLOSE_BEHAVIOR must be exit or relaunch, got ${closeBehavior}`,
    )
  }
  const platform = source.E2E_PLATFORM ?? 'linux'
  if (platform !== 'linux' && platform !== 'android') {
    throw new Error(`E2E_PLATFORM must be linux or android, got ${platform}`)
  }
  const timeScale = Number(source.E2E_TIME_SCALE ?? '1')
  if (!Number.isFinite(timeScale) || timeScale <= 0)
    throw new Error('E2E_TIME_SCALE must be a positive number')
  return {
    runDir: need('E2E_RUN_DIR'),
    app: need('E2E_APP'),
    closeBehavior,
    // An Android run has the Linux tools only where this machine can run the other devices of a
    // group (stage 2); without them a Linux device fails at its start with what is missing.
    tools:
      platform === 'android'
        ? linuxToolsOrNone(source.E2E_TOOLS)
        : toTools(JSON.parse(need('E2E_TOOLS'))),
    scenarioTimeoutMs: Number(source.E2E_SCENARIO_TIMEOUT_MS ?? '60000'),
    timeScale,
    marker: need('HOLZI_E2E_RUN'),
    keep: source.E2E_KEEP === '1',
    android:
      platform === 'android'
        ? {
            serial: need('ANDROID_SERIAL'),
            chromedriver: need('E2E_CHROMEDRIVER'),
            linuxApp: source.E2E_LINUX_APP || undefined,
          }
        : undefined,
  }
}

/** Linux tools an Android run does not have; a Linux device started with them fails at once. */
const NO_LINUX_TOOLS: Tools = {
  tauriDriver: 'tauri-driver (not available in this Android run)',
  webKitWebDriver: 'WebKitWebDriver (not available in this Android run)',
  xvfbRun: 'xvfb-run (not available in this Android run)',
}

function linuxToolsOrNone(found: string | undefined): Tools {
  try {
    return toTools(JSON.parse(found || '{}'))
  } catch {
    return NO_LINUX_TOOLS
  }
}

/** Declare a scenario. The name is the file's base name; the command checks that they agree. */
export function scenario(
  name: string,
  options: ScenarioOptions,
  body: (ctx: ScenarioContext) => Promise<void>,
): void {
  registerName(name)
  test(name, async (t) => {
    const env = readEnv()
    const android = env.android
    const adb = android === undefined ? undefined : createAdb(android.serial)
    const result = await runScenario(name, options, body, {
      env,
      startInstance: (request) =>
        android !== undefined && adb !== undefined
          ? startAndroidInstance({
              adb,
              chromedriver: android.chromedriver,
              marker: request.env.marker,
              logFile: request.logFile,
              colorScheme: request.colorScheme,
              reuse: request.reuse,
              phoneScreen: request.phoneScreen,
              withoutAllFiles: request.withoutAllFiles,
              step: request.step,
            })
          : startInstance({
              app: request.env.app,
              tools: request.env.tools,
              root: request.root,
              marker: request.env.marker,
              logFile: request.logFile,
              colorScheme: request.colorScheme,
              reuse: request.reuse,
              step: request.step,
              framebufferDir: request.framebufferDir,
            }),
      // The devices of a group run on Linux; in an Android run one of them runs on the phone.
      createHost: (request) =>
        createLinuxHost(
          android === undefined
            ? request
            : {
                ...request,
                env: {
                  ...request.env,
                  app:
                    android.linuxApp ??
                    'the Linux build (not available in this Android run)',
                },
              },
        ),
      createPhoneHost:
        android !== undefined && adb !== undefined
          ? (request) =>
              createAndroidHost({
                adb,
                chromedriver: android.chromedriver,
                marker: request.env.marker,
                logDir: join(request.env.runDir, request.scenario),
              })
          : undefined,
      onFailure: (info) =>
        captureFailure({
          runDir: info.env.runDir,
          scenario: info.scenario,
          error: info.error,
          steps: info.steps,
          failedStep: info.failedStep,
          deadlineMs: info.deadlineMs,
          instances: info.instances,
          providers: info.providers,
          devices: info.devices,
        }),
    })
    if (result.status === 'skipped') t.skip(result.skipReason)
    if (result.status === 'failed') throw new Error(result.error)
  })
}
