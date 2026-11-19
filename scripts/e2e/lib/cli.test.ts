import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import { EventEmitter } from 'node:events'
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  UsageError,
  findNameProblems,
  listScenarioNames,
  parseCliOptions,
  parseShard,
  runCli,
  selectShard,
} from '../cli.ts'
import type { CliDeps } from '../cli.ts'
import type { ScenarioResult } from './scenario.ts'

const FOUND = {
  'tauri-driver': '/t/tauri-driver',
  WebKitWebDriver: '/t/WebKitWebDriver',
  'xvfb-run': '/t/xvfb-run',
}

function writeResults(runDir: string, names: string[]) {
  mkdirSync(join(runDir, 'results'), { recursive: true })
  for (const name of names) {
    const result: ScenarioResult = {
      name,
      status: 'passed',
      durationMs: 100,
      steps: [],
    }
    writeFileSync(
      join(runDir, 'results', `${name}.json`),
      JSON.stringify(result),
    )
  }
}

function setup(overrides: Partial<CliDeps> = {}) {
  const root = mkdtempSync(join(tmpdir(), 'e2e-cli-'))
  const calls: string[] = []
  const lines: string[] = []
  const signals = new EventEmitter()
  const inputs: Parameters<CliDeps['runScenarios']>[0][] = []
  const deps: CliDeps = {
    repoRoot: '/repo',
    artifactsRoot: root,
    preflight: async () => {
      calls.push('preflight')
      return {
        ok: true,
        found: FOUND,
        tools: [{ name: 'tauri-driver', path: '/t/tauri-driver', ok: true }],
        versions: {},
        messages: [],
      }
    },
    scenarioNames: () => ['a', 'b', 'c'],
    scenarioNameProblems: () => [],
    newMarker: () => '1:2:abcd',
    newRunId: () => 'run-1',
    sweepOrphans: async () => {
      calls.push('sweep')
      return []
    },
    build: async () => void calls.push('build'),
    runScenarios: async (input) => {
      calls.push('scenarios')
      inputs.push(input)
      writeResults(input.runDir, input.names)
    },
    stopRun: async () => void calls.push('stopRun'),
    signals,
    print: (line) => void lines.push(line),
    ...overrides,
  }
  const cleanup = () => rmSync(root, { recursive: true, force: true })
  return { deps, calls, lines, signals, inputs, root, cleanup }
}

const waitForAbort = (signal: AbortSignal) =>
  new Promise<void>((resolve) => {
    if (signal.aborted) resolve()
    else signal.addEventListener('abort', () => resolve(), { once: true })
  })

describe('parseCliOptions', () => {
  it('defaults to a 60 s scenario limit and an 1800 s run limit', () => {
    const options = parseCliOptions([], {})
    assert.equal(options.scenarioTimeoutSec, 60)
    assert.equal(options.runTimeoutSec, 1800)
    assert.equal(options.keep, false)
    assert.equal(options.app, undefined)
  })

  it('reads the options and E2E_APP as the fallback for --app', () => {
    const options = parseCliOptions(
      [
        '--grep',
        'lock',
        '--keep',
        '--scenario-timeout',
        '30',
        '--run-timeout',
        '90',
        '--close-behavior',
        'relaunch',
      ],
      { E2E_APP: '/from/env' },
    )
    assert.deepEqual(options, {
      app: '/from/env',
      closeBehavior: 'relaunch',
      grep: 'lock',
      platform: 'linux',
      apk: undefined,
      shard: undefined,
      keep: true,
      scenarioTimeoutSec: 30,
      runTimeoutSec: 90,
      timeScale: 1,
    })
    assert.equal(
      parseCliOptions(['--app', '/flag'], { E2E_APP: '/from/env' }).app,
      '/flag',
    )
  })

  it('rejects what it cannot use', () => {
    assert.throws(
      () => parseCliOptions(['--scenario-timeout', '0'], {}),
      UsageError,
    )
    assert.throws(
      () => parseCliOptions(['--close-behavior', 'sideways'], {}),
      UsageError,
    )
    assert.throws(() => parseCliOptions(['--nope'], {}), UsageError)
    assert.throws(
      () => parseCliOptions([], { E2E_TIME_SCALE: '-1' }),
      UsageError,
    )
  })
})

describe('runCli', () => {
  it('works in the order preflight, sweep, build, scenarios, sweep by marker, and writes the report', async () => {
    const t = setup()
    try {
      const code = await runCli([], {}, t.deps)
      assert.equal(code, 0)
      assert.deepEqual(t.calls, [
        'preflight',
        'sweep',
        'build',
        'scenarios',
        'stopRun',
      ])
      assert.match(
        t.lines.join('\n'),
        /PASSED \(0 failed, 0 skipped, 3 passed\)/,
      )
      const report = JSON.parse(
        readFileSync(join(t.root, 'run-1', 'report.json'), 'utf8'),
      )
      assert.equal(report.status, 'passed')
    } finally {
      t.cleanup()
    }
  })

  it('skips the build for a given application, by flag or by E2E_APP', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'e2e-cli-app-'))
    try {
      mkdirSync(join(dir, 'target', 'release'), { recursive: true })
      const app = join(dir, 'target', 'release', 'haex')
      writeFileSync(app, '#!/bin/sh\n')
      chmodSync(app, 0o755)
      for (const [argv, env] of [
        [['--app', app], {}],
        [[], { E2E_APP: app }],
      ] as const) {
        const t = setup()
        try {
          assert.equal(await runCli([...argv], env, t.deps), 0)
          assert.ok(!t.calls.includes('build'))
          assert.equal(t.inputs[0].env.E2E_CLOSE_BEHAVIOR, 'relaunch')
          assert.equal(t.inputs[0].env.E2E_APP, app)
        } finally {
          t.cleanup()
        }
      }
    } finally {
      rmSync(dir, { recursive: true, force: true })
    }
  })

  it('starts nothing and exits 2 when the preflight fails', async () => {
    const t = setup({
      preflight: async () => ({
        ok: false,
        found: {},
        tools: [{ name: 'xvfb-run', ok: false }],
        versions: {},
        messages: ['xvfb-run is not on PATH: enter the dev shell'],
      }),
    })
    try {
      assert.equal(await runCli([], {}, t.deps), 2)
      assert.deepEqual(t.calls, [])
      assert.match(t.lines.join('\n'), /xvfb-run is not on PATH/)
      const report = JSON.parse(
        readFileSync(join(t.root, 'run-1', 'report.json'), 'utf8'),
      )
      assert.equal(report.status, 'preflight-failed')
    } finally {
      t.cleanup()
    }
  })

  it('exits 2 before sweeping or building when the close behavior of a given application is unknown', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'e2e-cli-app-'))
    const t = setup()
    try {
      const app = join(dir, 'haex')
      writeFileSync(app, '#!/bin/sh\n')
      chmodSync(app, 0o755)
      assert.equal(await runCli(['--app', app], {}, t.deps), 2)
      assert.deepEqual(t.calls, ['preflight'])
      assert.match(t.lines.join('\n'), /--close-behavior/)
    } finally {
      t.cleanup()
      rmSync(dir, { recursive: true, force: true })
    }
  })

  it('exits 2 when a scenario file does not declare the name of its file', async () => {
    const t = setup({ scenarioNameProblems: () => ['x.test.ts declares y'] })
    try {
      assert.equal(await runCli([], {}, t.deps), 2)
      assert.deepEqual(t.calls, [])
      assert.match(t.lines.join('\n'), /x\.test\.ts declares y/)
    } finally {
      t.cleanup()
    }
  })

  it('exits 3 and runs no scenario when the build fails', async () => {
    const t = setup({
      build: async () => {
        throw new Error('the build failed with exit status 101')
      },
    })
    try {
      assert.equal(await runCli([], {}, t.deps), 3)
      assert.ok(!t.calls.includes('scenarios'))
      assert.ok(t.calls.includes('stopRun'))
      assert.match(t.lines.join('\n'), /exit status 101/)
    } finally {
      t.cleanup()
    }
  })

  it('runs only the scenarios whose name contains the --grep text', async () => {
    const t = setup()
    try {
      assert.equal(await runCli(['--grep', 'b'], {}, t.deps), 0)
      assert.deepEqual(t.inputs[0].names, ['b'])
      assert.equal(t.inputs[0].grep, 'b')
    } finally {
      t.cleanup()
    }
  })

  it('reports leftovers of an earlier killed run before it starts', async () => {
    const t = setup({
      sweepOrphans: async () => [{ pid: 4242, marker: '9:9:dead', exe: null }],
    })
    try {
      await runCli([], {}, t.deps)
      assert.match(t.lines.join('\n'), /removed 1 leftover process.*4242/)
    } finally {
      t.cleanup()
    }
  })

  it('hands the scenario processes everything they read', async () => {
    const t = setup()
    try {
      await runCli(
        ['--keep', '--scenario-timeout', '30'],
        { E2E_TIME_SCALE: '2' },
        t.deps,
      )
      const env = t.inputs[0].env
      assert.equal(env.E2E_RUN_DIR, join(t.root, 'run-1'))
      assert.equal(env.E2E_APP, '/repo/src-tauri/target/debug/haex')
      assert.equal(env.E2E_CLOSE_BEHAVIOR, 'exit')
      assert.deepEqual(JSON.parse(env.E2E_TOOLS), FOUND)
      assert.equal(env.E2E_SCENARIO_TIMEOUT_MS, '30000')
      assert.equal(env.E2E_TIME_SCALE, '2')
      assert.equal(env.E2E_KEEP, '1')
      assert.equal(env.HAEX_E2E_RUN, '1:2:abcd')
      const report = JSON.parse(
        readFileSync(join(t.root, 'run-1', 'report.json'), 'utf8'),
      )
      assert.equal(report.conformance.status, 'non-conformant')
    } finally {
      t.cleanup()
    }
  })

  for (const [signal, code] of [
    ['SIGINT', 130],
    ['SIGTERM', 143],
  ] as const) {
    it(`stops the run's processes first and exits ${code} on ${signal}`, async () => {
      const t = setup({
        runScenarios: async (input) => {
          t.calls.push('scenarios')
          setTimeout(() => t.signals.emit(signal), 20)
          await waitForAbort(input.signal)
          t.calls.push('aborted')
        },
      })
      try {
        assert.equal(await runCli([], {}, t.deps), code)
        assert.deepEqual(t.calls, [
          'preflight',
          'sweep',
          'build',
          'scenarios',
          'aborted',
          'stopRun',
        ])
      } finally {
        t.cleanup()
      }
    })
  }

  it('at the run time limit fails the scenario in flight, skips the rest and exits 1', async () => {
    const t = setup({
      runScenarios: async (input) => {
        writeResults(input.runDir, ['a'])
        await waitForAbort(input.signal)
      },
    })
    try {
      assert.equal(await runCli(['--run-timeout', '0.05'], {}, t.deps), 1)
      const text = t.lines.join('\n')
      assert.match(text, /failed\s+b\s+.*run time limit/)
      assert.match(text, /skipped\s+c\s+run time limit/)
    } finally {
      t.cleanup()
    }
  })
})

describe('shards', () => {
  it('reads i/n and refuses anything else', () => {
    assert.deepEqual(parseShard('2/3'), { index: 2, total: 3 })
    for (const bad of ['0/3', '4/3', '1/0', 'x', '1-3']) {
      assert.throws(() => parseShard(bad), UsageError, bad)
    }
  })

  it('splits the scenarios so every one runs in exactly one shard', () => {
    const names = ['a', 'b', 'c', 'd', 'e']
    const parts = [1, 2, 3].map((index) =>
      selectShard(names, { index, total: 3 }),
    )
    assert.deepEqual(parts, [['a', 'd'], ['b', 'e'], ['c']])
    assert.deepEqual(parts.flat().sort(), names)
    assert.deepEqual(selectShard(names, undefined), names)
  })
})

describe('runCli on Android', () => {
  const androidPreflight = () => ({
    ok: true,
    messages: [],
    versions: { driver: 'ChromeDriver 124.0.0.1', webview: '124.0.0.2' },
    env: { ANDROID_SERIAL: 'emu', E2E_CHROMEDRIVER: '/cd' },
  })

  it('checks the device, builds the Linux app for the other devices and hands the scenarios both', async () => {
    const t = setup({
      androidPreflight,
      scenarioNames: () => ['a', 'extension-files', 'b'],
    })
    try {
      assert.equal(await runCli(['--platform', 'android'], {}, t.deps), 0)
      assert.deepEqual(t.calls, [
        'preflight',
        'sweep',
        'build',
        'scenarios',
        'stopRun',
      ])
      const input = t.inputs[0]!
      assert.deepEqual(input.names, ['a', 'b'])
      assert.equal(input.env.E2E_PLATFORM, 'android')
      assert.equal(input.env.ANDROID_SERIAL, 'emu')
      assert.equal(input.env.E2E_CHROMEDRIVER, '/cd')
      assert.equal(input.env.E2E_APP, 'com.haex.holzi')
      assert.equal(input.env.E2E_CLOSE_BEHAVIOR, 'exit')
      assert.equal(input.env.HAEX_E2E_SYNC_LOOPBACK, '1')
      assert.equal(input.env.E2E_LINUX_APP, '/repo/src-tauri/target/debug/haex')
      assert.deepEqual(JSON.parse(input.env.E2E_TOOLS!), FOUND)
      assert.match(
        t.lines.join('\n'),
        /not on android: extension-files \(excluded/,
      )
    } finally {
      t.cleanup()
    }
  })

  it('stops before scenarios when this machine lacks the Linux devices tools', async () => {
    const t = setup({
      androidPreflight,
      preflight: async () => {
        t.calls.push('preflight')
        return {
          ok: false,
          found: {},
          tools: [],
          versions: {},
          messages: ['tauri-driver is missing'],
        }
      },
    })
    try {
      assert.equal(await runCli(['--platform', 'android'], {}, t.deps), 2)
      assert.deepEqual(t.calls, ['preflight'])
      assert.equal(t.inputs.length, 0)
      assert.match(
        t.lines.join('\n'),
        /devices beside the phone cannot run here: tauri-driver is missing/,
      )
    } finally {
      t.cleanup()
    }
  })

  it('stops before any scenario when the device check fails', async () => {
    const t = setup({
      androidPreflight: () => ({
        ...androidPreflight(),
        ok: false,
        messages: ['adb sees no device'],
      }),
    })
    try {
      assert.equal(await runCli(['--platform', 'android'], {}, t.deps), 2)
      assert.deepEqual(t.calls, [])
      assert.match(t.lines.join('\n'), /adb sees no device/)
    } finally {
      t.cleanup()
    }
  })

  it('refuses --apk without --platform android', () => {
    assert.throws(
      () => parseCliOptions(['--apk', 'x.apk'], {}),
      /--apk needs --platform android/,
    )
  })
})

describe('the scenario files', () => {
  it('each declare the name of their file', () => {
    assert.deepEqual(findNameProblems(), [])
    assert.ok(listScenarioNames().length > 0)
  })
})
