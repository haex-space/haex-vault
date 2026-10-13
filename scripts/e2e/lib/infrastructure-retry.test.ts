import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import {
  brokenConnection,
  withConnectionRetry,
} from './infrastructure-retry.ts'
import type { ScenarioResult } from './scenario-types.ts'

const passed: ScenarioResult = {
  name: 'sync-copy',
  status: 'passed',
  durationMs: 1,
  steps: [],
}
const failed = (error: string): ScenarioResult => ({
  ...passed,
  status: 'failed',
  error,
})

/** The errors the CI's Android runs showed, word for word in the part that matters. */
const SEEN_ON_CI = [
  'POST /session/564f/execute/sync: disconnected: disconnected: Unable to receive message from renderer\n  (failed to check if window was closed: disconnected: not connected to DevTools)',
  'could not attach to the app on emulator-5554: chrome not reachable: chrome not reachable (see driver.log)',
  'chromedriver did not listen on port 45709 (see driver.log)',
  "teardown failed: Command failed: adb -s emulator-5554 shell pm clear com.haex.holzi\nadb: device 'emulator-5554' not found",
]

describe('infrastructure retry', () => {
  it('knows the broken connections seen on CI', () => {
    for (const error of SEEN_ON_CI) assert.ok(brokenConnection(error), error)
  })

  it('takes a failure of the scenario itself as it is', () => {
    for (const error of [
      'hook "files-entry-Archiv" (selector [data-testid="files-entry-Archiv"]) was not displayed within 120000 ms',
      "timed out after 30000 ms waiting for the device's files (last observed: false)",
      'process 16261 did not end within 4000 ms',
      undefined,
    ])
      assert.equal(brokenConnection(error), false, String(error))
  })

  it('runs once more after a broken connection and names the first error', async () => {
    const results = [failed(SEEN_ON_CI[1]!), passed]
    const written: ScenarioResult[] = []
    const result = await withConnectionRetry(
      () => Promise.resolve(results.shift()!),
      (r) => written.push(r),
    )
    assert.equal(result.status, 'passed')
    assert.equal(result.retriedAfter, SEEN_ON_CI[1])
    assert.deepEqual(written, [result])
    assert.equal(results.length, 0)
  })

  it('runs only once more, whatever the second run says', async () => {
    let runs = 0
    const result = await withConnectionRetry(
      () => {
        runs += 1
        return Promise.resolve(failed(SEEN_ON_CI[0]!))
      },
      () => {},
    )
    assert.equal(runs, 2)
    assert.equal(result.status, 'failed')
    assert.equal(result.retriedAfter, SEEN_ON_CI[0])
  })

  it('does not run again after a pass or any other failure', async () => {
    for (const first of [passed, failed('a wait timed out')]) {
      let runs = 0
      const result = await withConnectionRetry(
        () => {
          runs += 1
          return Promise.resolve(first)
        },
        () => assert.fail('nothing to write again'),
      )
      assert.equal(runs, 1)
      assert.equal(result, first)
    }
  })
})
