// One more run for a scenario whose first run failed. On a shared CI runner a scenario can fail for
// what is not holzi's doing: a click that did not land, a key the emulator lost, a renderer or a
// driver connection that went away. A second run says more about holzi than the first. The result
// names the first error and keeps its material, so every retry stays visible in the report and a
// flaky scenario can still be found and fixed; it is not hidden by the retry.
//
// Only a few scenarios of a run get the second run: when more than `RETRY_BUDGET` of them failed,
// the run is broken (the build, the emulator), not flaky, and running every failure twice would only
// make it end later.
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  renameSync,
  rmSync,
} from 'node:fs'
import { join } from 'node:path'
import type { ScenarioResult } from './scenario-types.ts'

/** How many scenarios of one run may be run a second time. */
export const RETRY_BUDGET = 3

/** How many scenarios of the run in `runDir` were run a second time so far (their result says so). */
export function retriesUsed(runDir: string): number {
  const dir = join(runDir, 'results')
  if (!existsSync(dir)) return 0
  let used = 0
  for (const file of readdirSync(dir)) {
    if (!file.endsWith('.json')) continue
    try {
      const result = JSON.parse(
        readFileSync(join(dir, file), 'utf8'),
      ) as Partial<ScenarioResult>
      if (result.retriedAfter !== undefined) used += 1
    } catch {
      // A result that is half written is no retry.
    }
  }
  return used
}

/**
 * Moves the material of a failed first run to `<runDir>/retried/<name>`, where the run's next one does
 * not overwrite it and a passing second run does not remove it. Returns the new place, or undefined
 * when the first run left none.
 */
export function keepFirstMaterial(
  runDir: string,
  name: string,
): string | undefined {
  const from = join(runDir, name)
  if (!existsSync(from)) return undefined
  const to = join(runDir, 'retried', name)
  mkdirSync(join(runDir, 'retried'), { recursive: true })
  rmSync(to, { recursive: true, force: true })
  renameSync(from, to)
  return to
}

export interface RetryOptions {
  /** How many scenarios of this run were run a second time already. */
  used: () => number
  /** How many may be; `RETRY_BUDGET` unless a check says otherwise. */
  budget?: number
  /** Called after a failed first run and before the second one; returns where its material went. */
  beforeRetry?: (first: ScenarioResult) => string | undefined
}

/**
 * Runs `run`, and once more if the first run failed and the run's budget of second runs is not used
 * up. The second result carries `retriedAfter` (the first error) and `retriedMaterial`, and is
 * written again with `write`. A pass, a skip and a second failure are taken as they are.
 */
export async function withRetry(
  run: () => Promise<ScenarioResult>,
  write: (result: ScenarioResult) => void,
  options: RetryOptions,
): Promise<ScenarioResult> {
  const first = await run()
  if (first.status !== 'failed') return first
  if (options.used() >= (options.budget ?? RETRY_BUDGET)) return first
  const material = options.beforeRetry?.(first)
  const second: ScenarioResult = {
    ...(await run()),
    retriedAfter: first.error ?? 'failed without a message',
  }
  if (material !== undefined) second.retriedMaterial = material
  write(second)
  return second
}
