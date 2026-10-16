import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  buildReport,
  exitCodeFor,
  annotations,
  formatSummary,
  readResults,
  writeReport,
} from './report.ts'
import type { RunFacts } from './report.ts'
import type { ScenarioResult } from './scenario.ts'

const application = {
  path: '/app/holzi',
  source: 'built' as const,
  closeBehavior: 'exit' as const,
  closeBehaviorFrom: 'path' as const,
}

function facts(extra: Partial<RunFacts> = {}): RunFacts {
  return {
    runId: '20260922-101500-ab12cd',
    marker: '1:2:abcd',
    application,
    tools: [{ name: 'tauri-driver', path: '/t/tauri-driver', ok: true }],
    versions: { driver: '2.52.6', webview: '2.52.6' },
    timeScale: 1,
    expected: ['a', 'b', 'c'],
    outcome: 'completed',
    ...extra,
  }
}

function result(
  name: string,
  status: ScenarioResult['status'],
  extra: Partial<ScenarioResult> = {},
): ScenarioResult {
  return { name, status, durationMs: 1500, steps: [], ...extra }
}

describe('buildReport', () => {
  it('passes only if no scenario failed; a skipped one does not fail the run', () => {
    const report = buildReport(facts(), [
      result('a', 'passed'),
      result('b', 'skipped', {
        skipReason:
          'the build exits on close; this scenario needs one that relaunches',
      }),
      result('c', 'passed'),
    ])
    assert.equal(report.status, 'passed')
    assert.equal(exitCodeFor(report), 0)
  })

  it('fails if any scenario failed', () => {
    const report = buildReport(facts(), [
      result('a', 'passed'),
      result('b', 'failed', { error: 'x' }),
      result('c', 'passed'),
    ])
    assert.equal(report.status, 'failed')
    assert.equal(exitCodeFor(report), 1)
  })

  it('counts a scenario file that wrote no result as failed', () => {
    const report = buildReport(facts(), [
      result('a', 'passed'),
      result('c', 'passed'),
    ])
    const missing = report.scenarios.find((s) => s.name === 'b')
    assert.equal(missing?.status, 'failed')
    assert.match(missing?.error ?? '', /no result written/)
    assert.equal(report.status, 'failed')
  })

  it('at the run limit fails the scenario in flight and skips the rest with the reason', () => {
    const report = buildReport(facts({ runLimitReached: true }), [
      result('a', 'passed'),
    ])
    const [, b, c] = report.scenarios
    assert.equal(b.status, 'failed')
    assert.match(b.error ?? '', /run time limit/)
    assert.equal(c.status, 'skipped')
    assert.equal(c.skipReason, 'run time limit')
    assert.equal(exitCodeFor(report), 1)
  })

  it('maps the other endings to their statuses and exit codes', () => {
    assert.equal(
      exitCodeFor(
        buildReport(facts({ outcome: 'preflight-failed', expected: [] }), []),
      ),
      2,
    )
    assert.equal(
      exitCodeFor(
        buildReport(facts({ outcome: 'build-failed', expected: [] }), []),
      ),
      3,
    )
    assert.equal(
      exitCodeFor(
        buildReport(
          facts({ outcome: 'interrupted', interruptedBy: 'SIGINT' }),
          [],
        ),
      ),
      130,
    )
    assert.equal(
      exitCodeFor(
        buildReport(
          facts({ outcome: 'interrupted', interruptedBy: 'SIGTERM' }),
          [],
        ),
      ),
      143,
    )
  })

  it('records the marker, the application and whether the run conforms', () => {
    const conformant = buildReport(facts(), [])
    assert.equal(conformant.runMarker, '1:2:abcd')
    assert.deepEqual(conformant.conformance, {
      status: 'conformant',
      timeScale: 1,
    })
    assert.deepEqual(conformant.application, application)
    const scaled = buildReport(facts({ timeScale: 2 }), [])
    assert.deepEqual(scaled.conformance, {
      status: 'non-conformant',
      timeScale: 2,
    })
  })
})

describe('formatSummary', () => {
  it('prints a line per scenario with status, name and duration, and the reason of a skip', () => {
    const report = buildReport(facts(), [
      result('a', 'passed', { durationMs: 9120 }),
      result('b', 'skipped', {
        skipReason:
          'the build exits on close; this scenario needs one that relaunches',
      }),
      result('c', 'failed', {
        durationMs: 14000,
        error: 'process still running',
      }),
    ])
    const text = formatSummary(report).join('\n')
    assert.match(text, /passed\s+a\s+9\.1 s/)
    assert.match(
      text,
      /skipped\s+b\s+the build exits on close; this scenario needs one that relaunches/,
    )
    assert.match(
      text,
      /failed\s+c\s+14\.0 s\s+failed at: process still running/,
    )
  })

  it('names the failed step and the material directory on their own line', () => {
    const report = buildReport(facts({ expected: ['a'] }), [
      result('a', 'failed', {
        durationMs: 4000,
        error: 'process 123 did not end within 4000 ms',
        failedStep: 'process-ended',
        material: '/run/a',
      }),
    ])
    const text = formatSummary(report).join('\n')
    assert.match(text, /failed\s+a\s+4\.0 s\s+failed at: process-ended/)
    assert.match(text, /material: \/run\/a/)
  })

  it('names the first error of a retried scenario on its own line', () => {
    const report = buildReport(facts({ expected: ['a', 'b'] }), [
      result('a', 'passed', {
        durationMs: 9100,
        retriedAfter: 'chrome not reachable\n  (details)',
        retriedMaterial: '/run/retried/a',
      }),
      result('b', 'failed', {
        durationMs: 4000,
        error: 'x',
        material: '/run/b',
        retriedAfter: 'not connected to DevTools',
      }),
    ])
    const lines = formatSummary(report)
    const after = (name: string) =>
      lines[lines.findIndex((line) => line.includes(` ${name} `)) + 1]
    assert.match(after('a') ?? '', /^\s+retried after: chrome not reachable$/)
    assert.match(
      lines.join('\n'),
      /retried after: chrome not reachable\n\s+first run's material: \/run\/retried\/a/,
    )
    assert.match(
      lines.join('\n'),
      /material: \/run\/b\n\s+retried after: not connected to DevTools/,
    )
  })

  it('annotates the scenarios that ran twice for GitHub', () => {
    const report = buildReport(facts({ expected: ['a', 'b', 'c'] }), [
      result('a', 'passed', {
        retriedAfter: 'hook "x" not displayed\n  (more)',
      }),
      result('b', 'failed', { error: 'y', retriedAfter: 'z' }),
      result('c', 'passed', { retriedAfter: '50% complete\rwhile loading' }),
    ])
    assert.deepEqual(annotations(report), [
      '::warning title=Scenario ran twice::a passed on the second run; the first run failed with: hook "x" not displayed',
      '::warning title=Scenario ran twice::b failed again; the first run failed with: z',
      '::warning title=Scenario ran twice::c passed on the second run; the first run failed with: 50%25 complete%0Dwhile loading',
    ])
  })

  it('shows press to process end for a passing scenario with both steps', () => {
    const report = buildReport(facts({ expected: ['a'] }), [
      result('a', 'passed', {
        durationMs: 9100,
        steps: [
          { name: 'press', atMs: 100, at: '2026-01-01T00:00:00.100Z' },
          {
            name: 'process-ended',
            atMs: 200,
            at: '2026-01-01T00:00:00.200Z',
          },
          { name: 'press', atMs: 6100, at: '2026-01-01T00:00:06.100Z' },
          {
            name: 'process-ended',
            atMs: 6700,
            at: '2026-01-01T00:00:06.700Z',
          },
        ],
      }),
    ])
    const text = formatSummary(report).join('\n')
    assert.match(text, /passed\s+a\s+9\.1 s\s+press to process end 0\.6 s/)
  })

  it('names the application, its close behavior and the totals', () => {
    const report = buildReport(facts(), [
      result('a', 'passed'),
      result('b', 'failed', { error: 'x' }),
      result('c', 'skipped', { skipReason: 'r' }),
    ])
    const last = formatSummary(report).join('\n')
    assert.match(
      last,
      /Application: \/app\/holzi \(closes by exit, from path\)/,
    )
    assert.match(last, /FAILED \(1 failed, 1 skipped, 1 passed\)/)
  })

  it('says when a run is not conformant', () => {
    const text = formatSummary(buildReport(facts({ timeScale: 3 }), [])).join(
      '\n',
    )
    assert.match(text, /non-conformant/)
    assert.match(text, /time scale 3/)
  })
})

describe('readResults and writeReport', () => {
  it('reads the result files of a run and writes report.json with the documented fields', () => {
    const dir = mkdtempSync(join(tmpdir(), 'e2e-report-'))
    try {
      mkdirSync(join(dir, 'results'))
      writeFileSync(
        join(dir, 'results', 'a.json'),
        JSON.stringify(result('a', 'passed')),
      )
      writeFileSync(join(dir, 'results', 'not-json.json'), '{')
      const results = readResults(dir)
      assert.deepEqual(
        results.map((r) => r.name),
        ['a'],
      )
      const report = buildReport(facts({ expected: ['a'] }), results)
      writeReport(dir, report)
      const written = JSON.parse(readFileSync(join(dir, 'report.json'), 'utf8'))
      for (const key of [
        'runId',
        'runMarker',
        'status',
        'application',
        'tools',
        'versions',
        'conformance',
        'scenarios',
      ]) {
        assert.ok(key in written, `report.json has ${key}`)
      }
      assert.equal(written.scenarios[0].name, 'a')
    } finally {
      rmSync(dir, { recursive: true, force: true })
    }
  })

  it('returns no results for a run directory without any', () => {
    const dir = mkdtempSync(join(tmpdir(), 'e2e-report-'))
    try {
      assert.deepEqual(readResults(dir), [])
    } finally {
      rmSync(dir, { recursive: true, force: true })
    }
  })
})
