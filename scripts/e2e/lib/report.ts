// The run's report: the scenario results in order, the application used, whether the run conforms, and
// the exit status (contracts/report.md, contracts/cli.md).
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { ApplicationInfo } from './build.ts'
import type { ToolFacts } from './preflight.ts'
import type { ScenarioResult, Step } from './scenario.ts'

export type RunOutcome =
  'completed' | 'preflight-failed' | 'build-failed' | 'interrupted'
export type RunStatus =
  'passed' | 'failed' | 'preflight-failed' | 'build-failed' | 'interrupted'

export interface RunFacts {
  runId: string
  marker: string
  /** Null when the run stopped before an application could be determined. */
  application: ApplicationInfo | null
  tools: ToolFacts[]
  versions: { driver?: string; webview?: string }
  timeScale: number
  /** The scenarios that were to run, in order. */
  expected: string[]
  outcome: RunOutcome
  interruptedBy?: 'SIGINT' | 'SIGTERM'
  runLimitReached?: boolean
}

/** `failedStep` and `material` are computed by `runScenario` itself, for a failed scenario only. */
export type ReportScenario = ScenarioResult

export interface Report {
  runId: string
  runMarker: string
  status: RunStatus
  interruptedBy?: 'SIGINT' | 'SIGTERM'
  application: ApplicationInfo | null
  tools: ToolFacts[]
  versions: { driver?: string; webview?: string }
  conformance: { status: 'conformant' | 'non-conformant'; timeScale: number }
  scenarios: ReportScenario[]
}

/** The result files the scenario processes wrote. A file that cannot be read counts as no result. */
export function readResults(runDir: string): ScenarioResult[] {
  let files: string[]
  try {
    files = readdirSync(join(runDir, 'results')).filter((file) =>
      file.endsWith('.json'),
    )
  } catch {
    return []
  }
  const results: ScenarioResult[] = []
  for (const file of files.sort()) {
    try {
      results.push(
        JSON.parse(
          readFileSync(join(runDir, 'results', file), 'utf8'),
        ) as ScenarioResult,
      )
    } catch {
      // Treated like a missing result below.
    }
  }
  return results
}

function withoutResult(
  name: string,
  status: 'failed' | 'skipped',
  reason: string,
): ReportScenario {
  return status === 'failed'
    ? { name, status, durationMs: 0, steps: [], error: reason }
    : { name, status, durationMs: 0, steps: [], skipReason: reason }
}

export function buildReport(
  facts: RunFacts,
  results: ScenarioResult[],
): Report {
  const byName = new Map(results.map((result) => [result.name, result]))
  const scenarios: ReportScenario[] = []
  if (facts.outcome === 'completed' || facts.outcome === 'interrupted') {
    let inFlightHandled = false
    for (const name of facts.expected) {
      const found = byName.get(name)
      if (found !== undefined) {
        scenarios.push(found)
      } else if (facts.runLimitReached === true) {
        // ponytail: scenarios run in the listed order, so the first one without a result is the one in
        // flight. Upgrade path if that stops holding: have each scenario process write a "started" file.
        scenarios.push(
          inFlightHandled
            ? withoutResult(name, 'skipped', 'run time limit')
            : withoutResult(name, 'failed', 'reached the run time limit'),
        )
        inFlightHandled = true
      } else if (facts.outcome === 'interrupted') {
        scenarios.push(withoutResult(name, 'skipped', 'run interrupted'))
      } else {
        scenarios.push(withoutResult(name, 'failed', 'no result written'))
      }
    }
    for (const result of results)
      if (!facts.expected.includes(result.name)) scenarios.push(result)
  }

  let status: RunStatus
  if (facts.outcome === 'completed')
    status = scenarios.some((s) => s.status === 'failed') ? 'failed' : 'passed'
  else status = facts.outcome

  return {
    runId: facts.runId,
    runMarker: facts.marker,
    status,
    ...(facts.interruptedBy === undefined
      ? {}
      : { interruptedBy: facts.interruptedBy }),
    application: facts.application,
    tools: facts.tools,
    versions: facts.versions,
    conformance: {
      status: facts.timeScale === 1 ? 'conformant' : 'non-conformant',
      timeScale: facts.timeScale,
    },
    scenarios,
  }
}

export function exitCodeFor(report: Report): number {
  switch (report.status) {
    case 'passed':
      return 0
    case 'failed':
      return 1
    case 'preflight-failed':
      return 2
    case 'build-failed':
      return 3
    case 'interrupted':
      return report.interruptedBy === 'SIGTERM' ? 143 : 130
  }
}

function seconds(ms: number): string {
  return `${(ms / 1000).toFixed(1)} s`
}

/** "press to process end 0.6 s", when both steps were recorded (contracts/report.md "Key steps"). */
function pressToEndText(steps: Step[]): string | undefined {
  const ended = [...steps].reverse().find((s) => s.name === 'process-ended')
  if (ended === undefined) return undefined
  const endedIndex = steps.lastIndexOf(ended)
  const press = steps
    .slice(0, endedIndex)
    .reverse()
    .find((s) => s.name === 'press')
  if (press === undefined) return undefined
  return `press to process end ${seconds(ended.atMs - press.atMs)}`
}

/**
 * GitHub Actions annotations for the scenarios that ran twice (`::warning …`), so a flaky scenario
 * shows on the run's page even when the run is green. Printed by the run itself: from a scenario's
 * own process the test runner would put `# ` before the command and GitHub would not see it.
 */
export function annotations(report: Report): string[] {
  return report.scenarios
    .filter((s) => s.retriedAfter !== undefined)
    .map((s) => {
      const first = (s.retriedAfter ?? '')
        .split('\n')[0]
        .replaceAll('%', '%25')
        .replaceAll('\r', '%0D')
      const outcome =
        s.status === 'passed' ? 'passed on the second run' : 'failed again'
      return `::warning title=Scenario ran twice::${s.name} ${outcome}; the first run failed with: ${first}`
    })
}

export function formatSummary(report: Report): string[] {
  const width = Math.max(0, ...report.scenarios.map((s) => s.name.length))
  const lines: string[] = []
  for (const s of report.scenarios) {
    const head = `${s.status.padEnd(7)} ${s.name.padEnd(width)}`
    if (s.status === 'skipped') {
      lines.push(`${head}   ${s.skipReason ?? ''}`)
      continue
    }
    const time = seconds(s.durationMs).padStart(7)
    const indent = ' '.repeat(head.length + 1 + time.length)
    const retried =
      s.retriedAfter === undefined
        ? []
        : [
            `${indent}   retried after: ${s.retriedAfter.split('\n')[0]}`,
            ...(s.retriedMaterial === undefined
              ? []
              : [`${indent}   first run's material: ${s.retriedMaterial}`]),
          ]
    if (s.status === 'failed') {
      lines.push(
        `${head} ${time}   failed at: ${s.failedStep ?? s.error ?? ''}`,
      )
      if (s.material !== undefined) {
        lines.push(`${indent}   material: ${s.material}`)
      }
      lines.push(...retried)
      continue
    }
    const pressToEnd = pressToEndText(s.steps)
    lines.push(
      `${head} ${time}${pressToEnd === undefined ? '' : `   ${pressToEnd}`}`,
      ...retried,
    )
  }
  const count = (status: string) =>
    report.scenarios.filter((s) => s.status === status).length
  const result =
    report.status === 'passed' || report.status === 'failed'
      ? `${report.status === 'passed' ? 'PASSED' : 'FAILED'} (${count('failed')} failed, ${count('skipped')} skipped, ${count('passed')} passed)`
      : report.status.replace('-', ' ').toUpperCase()
  const app = report.application
  const named =
    app === null
      ? '(not determined)'
      : `${app.path} (closes by ${app.closeBehavior}, from ${app.closeBehaviorFrom})`
  lines.push('', `Application: ${named}   Result: ${result}`)
  if (report.conformance.status === 'non-conformant') {
    lines.push(
      `This run is non-conformant (time scale ${report.conformance.timeScale}): generic timeouts were stretched.`,
    )
  }
  return lines
}

export function writeReport(runDir: string, report: Report): void {
  mkdirSync(runDir, { recursive: true })
  writeFileSync(join(runDir, 'report.json'), JSON.stringify(report, null, 2))
}
