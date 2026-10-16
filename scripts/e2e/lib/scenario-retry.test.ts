import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import {
  mkdtempSync,
  mkdirSync,
  rmSync,
  writeFileSync,
  existsSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  RETRY_BUDGET,
  keepFirstMaterial,
  retriesUsed,
  withRetry,
} from './scenario-retry.ts'
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
const unused = { used: () => 0 }

describe('scenario retry', () => {
  it('runs once more after any failure and names the first error', async () => {
    for (const error of [
      'POST /session/564f/execute/sync: disconnected: Unable to receive message from renderer',
      'hook "appearance-reset-confirm" was not displayed within 5000 ms',
      'timed out after 10000 ms waiting for the window background to change',
    ]) {
      const results = [failed(error), passed]
      const written: ScenarioResult[] = []
      const result = await withRetry(
        () => Promise.resolve(results.shift()!),
        (r) => written.push(r),
        unused,
      )
      assert.equal(result.status, 'passed')
      assert.equal(result.retriedAfter, error)
      assert.deepEqual(written, [result])
      assert.equal(results.length, 0)
    }
  })

  it('runs only once more, whatever the second run says', async () => {
    let runs = 0
    const result = await withRetry(
      () => {
        runs += 1
        return Promise.resolve(failed(`failure ${runs}`))
      },
      () => {},
      unused,
    )
    assert.equal(runs, 2)
    assert.equal(result.status, 'failed')
    assert.equal(result.error, 'failure 2')
    assert.equal(result.retriedAfter, 'failure 1')
  })

  it('does not run again after a pass or a skip', async () => {
    for (const first of [
      passed,
      { ...passed, status: 'skipped' as const, skipReason: 'no phone' },
    ]) {
      let runs = 0
      const result = await withRetry(
        () => {
          runs += 1
          return Promise.resolve(first)
        },
        () => assert.fail('nothing to write again'),
        unused,
      )
      assert.equal(runs, 1)
      assert.equal(result, first)
    }
  })

  it('does not run again once the run has used its budget', async () => {
    let runs = 0
    const result = await withRetry(
      () => {
        runs += 1
        return Promise.resolve(failed('broken'))
      },
      () => assert.fail('nothing to write again'),
      { used: () => RETRY_BUDGET },
    )
    assert.equal(runs, 1)
    assert.equal(result.retriedAfter, undefined)
    assert.equal(
      (
        await withRetry(
          () => Promise.resolve(failed('x')),
          () => {},
          { used: () => RETRY_BUDGET - 1 },
        )
      ).retriedAfter,
      'x',
    )
  })

  it('keeps the first run material and names where it is', async () => {
    const calls: string[] = []
    const results = [failed('first'), passed]
    const result = await withRetry(
      () => {
        calls.push('run')
        return Promise.resolve(results.shift()!)
      },
      () => {},
      {
        used: () => 0,
        beforeRetry: (first) => {
          calls.push(`keep ${first.error}`)
          return '/run/retried/sync-copy'
        },
      },
    )
    assert.deepEqual(calls, ['run', 'keep first', 'run'])
    assert.equal(result.retriedMaterial, '/run/retried/sync-copy')
  })
})

describe('retries of a run', () => {
  const withRunDir = (body: (runDir: string) => void) => {
    const runDir = mkdtempSync(join(tmpdir(), 'e2e-retry-'))
    try {
      body(runDir)
    } finally {
      rmSync(runDir, { recursive: true, force: true })
    }
  }

  it('counts the results that say they were retried', () => {
    withRunDir((runDir) => {
      assert.equal(retriesUsed(runDir), 0)
      mkdirSync(join(runDir, 'results'))
      const write = (name: string, result: object) =>
        writeFileSync(
          join(runDir, 'results', `${name}.json`),
          JSON.stringify(result),
        )
      write('a', passed)
      write('b', { ...passed, retriedAfter: 'x' })
      write('c', { ...failed('y'), retriedAfter: 'z' })
      writeFileSync(join(runDir, 'results', 'd.json'), '{ half')
      assert.equal(retriesUsed(runDir), 2)
    })
  })

  it('moves the material of the first run out of the way', () => {
    withRunDir((runDir) => {
      assert.equal(keepFirstMaterial(runDir, 'a'), undefined)
      mkdirSync(join(runDir, 'a'))
      writeFileSync(join(runDir, 'a', 'driver.log'), 'first')
      const kept = keepFirstMaterial(runDir, 'a')
      assert.equal(kept, join(runDir, 'retried', 'a'))
      assert.ok(existsSync(join(kept!, 'driver.log')))
      assert.equal(existsSync(join(runDir, 'a')), false)
    })
  })
})
