// What a failed scenario leaves behind, so a maintainer sees why without a second run (FR-019, FR-020,
// SC-004). `driver.log` needs no writing here: it already exists, written continuously for the life of
// each instance (scripts/e2e/lib/instance.ts); this only adds the three files a failure specifically
// needs.
import { mkdirSync, readdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { CaptureDevice } from './group.ts'
import type { Instance } from './instance.ts'
import { formatPageState, PAGE_STATE_SCRIPT } from './page-state.ts'
import type { PageState } from './page-state.ts'
import type { Provider } from './provider.ts'
import type { Step } from './scenario.ts'

const FAILURE_SCREENSHOT_LIMIT_MS = 5_000

export interface CaptureFailureOptions {
  runDir: string
  scenario: string
  error: unknown
  steps: Step[]
  failedStep?: string
  deadlineMs?: number
  instances: Instance[]
  providers: Provider[]
  /** The devices of a group, each with a folder of its own (contracts/failure-material.md). */
  devices?: CaptureDevice[]
}

function describeError(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

/**
 * Takes a screenshot of the first instance the scenario started, unless the application had already
 * ended (no window left to capture) or the session is gone by the time this runs; either way this never
 * throws, since a diagnostic must never hide the failure it describes.
 */
async function screenshotOrNote(
  dir: string,
  instances: Instance[],
): Promise<string | undefined> {
  const instance = instances[0]
  if (instance === undefined) return undefined
  if (!instance.alive()) {
    return 'no screenshot: the application had already ended'
  }
  try {
    writeFileSync(
      join(dir, 'screenshot.png'),
      await instance.screenshot(FAILURE_SCREENSHOT_LIMIT_MS),
    )
    return undefined
  } catch (error) {
    return `no screenshot: ${describeError(error)}`
  }
}

/**
 * `page-state.txt` (the address, the focus, the dialogs, the test ids and the text of the page) and,
 * on a phone, `logcat.txt`, for the first instance. A part that cannot be taken leaves its reason in
 * the file instead of nothing, and neither throws.
 */
function captureWhatItSaw(dir: string, instances: Instance[]): Promise<void> {
  return (async () => {
    const instance = instances[0]
    if (instance === undefined) return
    let page: string
    if (!instance.alive()) {
      page = 'no page state: the application had already ended\n'
    } else {
      try {
        let timer: ReturnType<typeof setTimeout> | undefined
        const state = await Promise.race([
          instance.exec<PageState>(PAGE_STATE_SCRIPT),
          new Promise<never>((_, reject) => {
            timer = setTimeout(
              () => reject(new Error('the page did not answer in time')),
              FAILURE_SCREENSHOT_LIMIT_MS,
            )
          }),
        ]).finally(() => clearTimeout(timer))
        page = formatPageState(state)
      } catch (error) {
        page = `no page state: ${describeError(error)}\n`
      }
    }
    writeFileSync(join(dir, 'page-state.txt'), page)
    if (instance.phone === undefined) return
    try {
      writeFileSync(
        join(dir, 'logcat.txt'),
        instance.phone.logTail(instance.pid),
      )
    } catch (error) {
      writeFileSync(
        join(dir, 'logcat.txt'),
        `no log: ${describeError(error)}\n`,
      )
    }
  })()
}

/**
 * One folder per device of a group: a screenshot while the device lives (a note when it does not) and
 * a copy of its data. A device that fails to capture leaves a note and never stops the others.
 * Returns the files each folder holds, for the timeline.
 */
async function captureDevices(
  dir: string,
  devices: CaptureDevice[],
): Promise<Array<{ folder: string; files: string[] }>> {
  const kept: Array<{ folder: string; files: string[] }> = []
  for (const device of devices) {
    const deviceDir = join(dir, device.folder)
    try {
      mkdirSync(deviceDir, { recursive: true })
      if (device.alive()) {
        try {
          writeFileSync(
            join(deviceDir, 'screenshot.png'),
            await device.screenshot(FAILURE_SCREENSHOT_LIMIT_MS),
          )
        } catch (error) {
          writeFileSync(
            join(deviceDir, 'screenshot-note.txt'),
            `no screenshot: ${describeError(error)}\n`,
          )
        }
      } else {
        writeFileSync(
          join(deviceDir, 'screenshot-note.txt'),
          'no screenshot: the device was not running\n',
        )
      }
      try {
        device.keepData(deviceDir)
      } catch (error) {
        writeFileSync(
          join(deviceDir, 'data-note.txt'),
          `no data kept: ${describeError(error)}\n`,
        )
      }
      kept.push({ folder: device.folder, files: readdirSync(deviceDir).sort() })
    } catch {
      // Best effort, as the rest of the capture.
    }
  }
  return kept
}

/**
 * Writes `timeline.json`, `screenshot.png` and `provider.json` into the scenario's own directory under
 * the run directory (and `page-state.txt`, on a phone also `logcat.txt`). Never throws: a diagnostic that fails must not hide the failure it was meant to
 * explain.
 */
export async function captureFailure(
  options: CaptureFailureOptions,
): Promise<void> {
  const dir = join(options.runDir, options.scenario)
  try {
    mkdirSync(dir, { recursive: true })
    const screenshotNote = await screenshotOrNote(dir, options.instances)
    await captureWhatItSaw(dir, options.instances)
    const devices = await captureDevices(dir, options.devices ?? [])
    writeFileSync(
      join(dir, 'timeline.json'),
      JSON.stringify(
        {
          steps: options.steps,
          error: describeError(options.error),
          ...(options.failedStep === undefined
            ? {}
            : { failedStep: options.failedStep }),
          ...(options.deadlineMs === undefined
            ? {}
            : { deadlineMs: options.deadlineMs }),
          ...(screenshotNote === undefined ? {} : { screenshotNote }),
          ...(devices.length === 0 ? {} : { devices }),
        },
        null,
        2,
      ),
    )
    writeFileSync(
      join(dir, 'provider.json'),
      JSON.stringify(
        options.providers.map((provider) => ({
          connections: provider.connections(),
          requests: provider.requests(),
        })),
        null,
        2,
      ),
    )
  } catch {
    // Best effort: a scenario's real failure must still be reported even if capturing it fails too.
  }
}
