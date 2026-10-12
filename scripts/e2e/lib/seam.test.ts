// SC-005: scenarios and the helpers they use name no platform. Everything platform specific lives in
// `lib/platform/` (the driver layer) or in the harness that starts a run; the text scan below is what
// keeps it so, until each exception is gone.
import assert from 'node:assert/strict'
import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { describe, it } from 'node:test'
import { fileURLToPath } from 'node:url'
import { findSeamViolations } from './seam.testlib.ts'

const HERE = dirname(fileURLToPath(import.meta.url))
const E2E = join(HERE, '..')

/** Helpers a scenario imports; the rest of `lib/` is the harness or the driver layer. */
const SCENARIO_FACING = [
  'lib/sync-flows.ts',
  'lib/flows.ts',
  'lib/settings.ts',
  'lib/group.ts',
  'lib/device.ts',
  'lib/group-plan.ts',
  'lib/group-expect.ts',
  'lib/sync-ui.ts',
  'lib/device-folder.ts',
]

/**
 * Files that still name a platform, each with why. An entry whose file no longer offends fails the
 * check, so this list can only shrink.
 */
const KNOWN_VIOLATIONS: Record<string, string> = {
  'scenarios/relaunch-after-lock.test.ts':
    'checks the window server of the virtual screen through its framebuffer; a process-level scenario of spec 013, not a multi-device one',
}

const scenarioFiles = () =>
  readdirSync(join(E2E, 'scenarios'))
    .filter((file) => file.endsWith('.test.ts'))
    .map((file) => `scenarios/${file}`)

describe('the seam between scenarios and the platform', () => {
  it('lists helpers that exist', () => {
    for (const file of SCENARIO_FACING) {
      assert.ok(existsSync(join(E2E, file)), `${file} is listed but missing`)
    }
    for (const file of Object.keys(KNOWN_VIOLATIONS)) {
      assert.ok(existsSync(join(E2E, file)), `${file} is exempt but missing`)
    }
  })

  it('has no scenario or scenario-facing helper that names a platform, except the listed ones', () => {
    const offenders: string[] = []
    for (const file of [...scenarioFiles(), ...SCENARIO_FACING]) {
      if (file in KNOWN_VIOLATIONS) continue
      const found = findSeamViolations(
        readFileSync(join(E2E, file), 'utf8'),
        file,
      )
      for (const line of found) offenders.push(`${file}: ${line}`)
    }
    assert.deepEqual(offenders, [])
  })

  it('keeps no exemption for a file that no longer offends', () => {
    for (const file of Object.keys(KNOWN_VIOLATIONS)) {
      const found = findSeamViolations(
        readFileSync(join(E2E, file), 'utf8'),
        file,
      )
      assert.ok(
        found.length > 0,
        `${file} is exempt but no longer offends; remove it from the list`,
      )
    }
  })

  it('catches each kind of platform specific', () => {
    const cases: Array<[string, string, RegExp]> = [
      [
        "import { startInstance } from '../lib/instance.ts'",
        'scenarios/example.ts',
        /platform file/,
      ],
      [
        "import { stopGroup } from './processes.ts'",
        'lib/example.ts',
        /platform file/,
      ],
      [
        "import type { X } from './webdriver.ts'",
        'lib/example.ts',
        /platform file/,
      ],
      [
        "import { startInstance } from './platform/../instance.ts'",
        'lib/example.ts',
        /platform file/,
      ],
      [
        "import { spawn } from 'node:child_process'",
        'scenarios/example.ts',
        /node:child_process/,
      ],
      ["import { tmpdir } from 'node:os'", 'scenarios/example.ts', /node:os/],
      [
        "import { copyFileSync } from 'node:fs'",
        'scenarios/example.ts',
        /node:fs/,
      ],
      ['process.kill(pid, 0)', 'scenarios/example.ts', /process\.kill/],
      ["const cmd = 'xvfb-run'", 'scenarios/example.ts', /xvfb/],
      ["run('tauri-driver')", 'scenarios/example.ts', /tauri-driver/],
      ["readFileSync('/proc/1/environ')", 'scenarios/example.ts', /\/proc/],
    ]
    for (const [source, file, expected] of cases) {
      const found = findSeamViolations(source, file)
      assert.equal(found.length, 1, source)
      assert.match(found[0] ?? '', expected)
    }
    assert.deepEqual(
      findSeamViolations(
        "import { startInstance } from '../other/instance.ts'",
        'scenarios/example.ts',
      ),
      [],
    )
  })

  it('ignores comments and the driver layer types', () => {
    const source = [
      '// the driver layer hides xvfb and tauri-driver',
      '/* process.kill is used in /proc below',
      '   node:fs too */',
      "import type { InvokeResult } from '../lib/platform/host.ts'",
      "const url = 'https://example.test/a//b' // trailing comment about xvfb",
    ].join('\n')
    assert.deepEqual(findSeamViolations(source, 'scenarios/example.ts'), [])
  })
})
