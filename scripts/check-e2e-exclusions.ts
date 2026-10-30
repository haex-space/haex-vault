// `pnpm check:e2e-exclusions` (spec 043 FR-033b, SC-007): the scenarios that do not run on Android
// are listed with a reason, each final exclusion names its counter case, the share that runs is at
// least the minimum, and from stage 5 on nothing is left pending.
import assert from 'node:assert/strict'
import { readdirSync } from 'node:fs'
import { test } from 'node:test'
import {
  EXCLUSIONS,
  MINIMUM_COVERAGE,
  PENDING_ALLOWED,
} from './e2e/platform-exclusions.ts'
import type { PlatformExclusion } from './e2e/platform-exclusions.ts'

const scenarios = readdirSync('scripts/e2e/scenarios')
  .filter((file) => file.endsWith('.test.ts'))
  .map((file) => file.slice(0, -'.test.ts'.length))

/** Scenarios that only exist for Android; they are not desktop scenarios and do not count. */
const androidOnly = (name: string) => name.startsWith('android-')

/** The share of desktop scenarios that run on Android. */
export function coverage(
  names: string[],
  entries: PlatformExclusion[],
): { runs: number; total: number; share: number } {
  const desktop = names.filter((name) => !androidOnly(name))
  const skipped = new Set(
    entries.filter((e) => e.platform === 'android').map((e) => e.scenario),
  )
  const runs = desktop.filter((name) => !skipped.has(name)).length
  return { runs, total: desktop.length, share: runs / desktop.length }
}

/** What is wrong with the list; empty when it follows the rules. */
export function problems(
  names: string[],
  entries: PlatformExclusion[],
  pendingAllowed: boolean,
): string[] {
  const found: string[] = []
  const seen = new Set<string>()
  for (const entry of entries) {
    const key = `${entry.platform}:${entry.scenario}`
    if (seen.has(key)) found.push(`${key} is listed twice`)
    seen.add(key)
    if (!names.includes(entry.scenario))
      found.push(`${entry.scenario} is no scenario`)
    if (entry.reason.trim() === '')
      found.push(`${entry.scenario} gives no reason`)
    if (entry.kind === 'excluded') {
      if (!/FR-0(06|16)/.test(entry.reason))
        found.push(`${entry.scenario}: an exclusion names FR-006 or FR-016`)
      if (!pendingAllowed && !names.includes(entry.counterCase))
        found.push(
          `${entry.scenario}: counter case ${entry.counterCase} is no scenario`,
        )
    } else if (entry.kind === 'pending') {
      if (entry.stage.trim() === '')
        found.push(`${entry.scenario} is pending without a stage`)
      if (!pendingAllowed)
        found.push(`${entry.scenario} is still pending (stage ${entry.stage})`)
    } else if (!pendingAllowed && !names.includes(entry.counterCase)) {
      found.push(
        `${entry.scenario}: counter case ${entry.counterCase} is no scenario`,
      )
    }
  }
  return found
}

test('the exclusion list follows the rules of FR-033b', () => {
  assert.deepEqual(problems(scenarios, EXCLUSIONS, PENDING_ALLOWED), [])
})

test('at least the minimum share of the desktop scenarios runs on Android', () => {
  const { runs, total, share } = coverage(scenarios, EXCLUSIONS)
  console.log(
    `android: ${runs} of ${total} desktop scenarios run (${Math.round(share * 100)} %), minimum ${MINIMUM_COVERAGE * 100} %`,
  )
  if (!PENDING_ALLOWED) assert.ok(share >= MINIMUM_COVERAGE)
})

test('the rules catch each kind of mistake', () => {
  const names = ['a', 'b', 'c']
  const excluded = (scenario: string, reason: string, counterCase = 'b') =>
    ({
      scenario,
      platform: 'android',
      kind: 'excluded',
      reason,
      counterCase,
    }) as const
  const pending = (scenario: string, stage: string) =>
    ({
      scenario,
      platform: 'android',
      kind: 'pending',
      stage,
      reason: 'later',
    }) as const
  const harnessLimited = (scenario: string, counterCase: string) =>
    ({
      scenario,
      platform: 'android',
      kind: 'harness-limited',
      reason: 'the runner cannot pass its local fixture to the app process',
      counterCase,
    }) as const
  assert.deepEqual(
    problems(names, [excluded('a', 'no shell (FR-016)')], true),
    [],
  )
  assert.match(
    problems(names, [excluded('z', 'x (FR-016)')], true).join(),
    /no scenario/,
  )
  assert.match(
    problems(names, [excluded('a', 'no reason given')], true).join(),
    /FR-006 or FR-016/,
  )
  assert.match(
    problems(names, [excluded('a', 'x (FR-016)', 'gone')], false).join(),
    /counter case gone/,
  )
  assert.deepEqual(problems(names, [harnessLimited('a', 'b')], false), [])
  assert.match(
    problems(names, [harnessLimited('a', 'gone')], false).join(),
    /counter case gone/,
  )
  assert.match(
    problems(names, [pending('a', '')], true).join(),
    /without a stage/,
  )
  assert.match(
    problems(names, [pending('a', '2')], false).join(),
    /still pending/,
  )
  assert.deepEqual(coverage(['a', 'b', 'android-x'], [pending('a', '2')]), {
    runs: 1,
    total: 2,
    share: 0.5,
  })
})
