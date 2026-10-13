// One more run for a scenario whose first run the connection to the phone broke: the adb transport to
// the emulator dropped, or chromedriver lost or never reached the app's web view. Seen on CI in
// scenarios whose app kept running (logcat: adbd "connection terminated", the app still drawing), so
// a second run says more about holzi than the first. Any other failure counts at once, and a second
// failure counts as it is; the result names the first error, so the report shows every retry.
import type { ScenarioResult } from './scenario-types.ts'

/** What chromedriver and adb answer when the way to the phone broke, not the app. */
const BROKEN_CONNECTION = [
  /Unable to receive message from renderer/,
  /not connected to DevTools/,
  /chrome not reachable/,
  /chromedriver did not listen on port/,
  /adb: device '[^']*' not found/,
]

/** Whether `error` says the connection to the phone broke. */
export function brokenConnection(error: string | undefined): boolean {
  return (
    error !== undefined &&
    BROKEN_CONNECTION.some((pattern) => pattern.test(error))
  )
}

/**
 * Runs `run`, and once more if the first run failed because the connection to the phone broke. The
 * second result carries `retriedAfter` and is written again with `write`.
 */
export async function withConnectionRetry(
  run: () => Promise<ScenarioResult>,
  write: (result: ScenarioResult) => void,
): Promise<ScenarioResult> {
  const first = await run()
  if (first.status !== 'failed' || !brokenConnection(first.error)) return first
  const second = { ...(await run()), retriedAfter: first.error }
  write(second)
  return second
}
