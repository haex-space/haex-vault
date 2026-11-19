import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  BuildError,
  buildDebugApplication,
  classifyCloseBehavior,
  debugBinaryPath,
  resolveApplication,
  targetDirectory,
} from './build.ts'
import type { BuildRunner } from './build.ts'

describe('the target directory and the debug binary', () => {
  it('is src-tauri/target unless CARGO_TARGET_DIR says otherwise', () => {
    assert.equal(targetDirectory('/repo', {}), '/repo/src-tauri/target')
    assert.equal(
      targetDirectory('/repo', { CARGO_TARGET_DIR: '/cache/target' }),
      '/cache/target',
    )
    // Cargo runs in src-tauri, so a relative directory is relative to it.
    assert.equal(
      targetDirectory('/repo', { CARGO_TARGET_DIR: 'out' }),
      '/repo/src-tauri/out',
    )
  })

  it('names the debug binary haex', () => {
    assert.equal(
      debugBinaryPath('/repo', {}),
      '/repo/src-tauri/target/debug/haex',
    )
  })
})

describe('classifyCloseBehavior', () => {
  it('reads the behavior from the profile directory in the path', () => {
    assert.deepEqual(classifyCloseBehavior('/x/target/debug/haex'), {
      closeBehavior: 'exit',
      from: 'path',
    })
    assert.deepEqual(classifyCloseBehavior('/x/target/release/haex'), {
      closeBehavior: 'relaunch',
      from: 'path',
    })
    assert.deepEqual(
      classifyCloseBehavior('/x/target/aarch64-apple-darwin/release/haex'),
      {
        closeBehavior: 'relaunch',
        from: 'path',
      },
    )
  })

  it('takes an explicit value when the path says nothing', () => {
    assert.deepEqual(classifyCloseBehavior('/x/out/haex', 'relaunch'), {
      closeBehavior: 'relaunch',
      from: 'flag',
    })
  })

  it('fails and says how to state the behavior when neither the path nor a flag does', () => {
    assert.throws(
      () => classifyCloseBehavior('/x/out/haex'),
      /--close-behavior/,
    )
    assert.throws(
      () => classifyCloseBehavior('/x/debug/release/haex'),
      /--close-behavior/,
    )
  })

  it('rejects an explicit value that conflicts with the path', () => {
    assert.throws(
      () => classifyCloseBehavior('/x/target/debug/haex', 'relaunch'),
      /conflicts/,
    )
    assert.throws(
      () => classifyCloseBehavior('/x/target/release/haex', 'exit'),
      /conflicts/,
    )
  })

  it('reports the flag as the source when it agrees with the path', () => {
    assert.deepEqual(classifyCloseBehavior('/x/target/debug/haex', 'exit'), {
      closeBehavior: 'exit',
      from: 'flag',
    })
  })
})

describe('resolveApplication', () => {
  function withApp(profile: string, mode: number, fn: (path: string) => void) {
    const dir = mkdtempSync(join(tmpdir(), 'e2e-build-'))
    try {
      mkdirSync(join(dir, 'target', profile), { recursive: true })
      const path = join(dir, 'target', profile, 'haex')
      writeFileSync(path, '#!/bin/sh\n')
      chmodSync(path, mode)
      fn(path)
    } finally {
      rmSync(dir, { recursive: true, force: true })
    }
  }

  it('describes the debug build the suite makes itself when no path is given', () => {
    const app = resolveApplication({ repoRoot: '/repo', env: {} })
    assert.deepEqual(app, {
      path: '/repo/src-tauri/target/debug/haex',
      source: 'built',
      closeBehavior: 'exit',
      closeBehaviorFrom: 'path',
    })
  })

  it('takes a given path as it is, checks it and reads its close behavior', () => {
    withApp('release', 0o755, (path) => {
      const app = resolveApplication({
        repoRoot: '/repo',
        env: {},
        appPath: path,
      })
      assert.deepEqual(app, {
        path,
        source: 'given',
        closeBehavior: 'relaunch',
        closeBehaviorFrom: 'path',
      })
    })
  })

  it('fails for a path that does not exist or cannot be run', () => {
    assert.throws(
      () =>
        resolveApplication({
          repoRoot: '/repo',
          env: {},
          appPath: '/no/such/haex',
        }),
      /does not exist/,
    )
    withApp('debug', 0o644, (path) => {
      assert.throws(
        () => resolveApplication({ repoRoot: '/repo', env: {}, appPath: path }),
        /not executable/,
      )
    })
  })
})

describe('buildDebugApplication', () => {
  it('runs the debug build without bundling in the repository root', async () => {
    const calls: Array<{ command: string; args: string[]; cwd: string }> = []
    const run: BuildRunner = async (command, args, options) => {
      calls.push({ command, args, cwd: options.cwd })
      return 0
    }
    await buildDebugApplication({
      repoRoot: '/repo',
      logFile: '/log/build.log',
      marker: 'm',
      run,
    })
    assert.deepEqual(calls, [
      {
        command: 'pnpm',
        args: ['tauri', 'build', '--debug', '--no-bundle'],
        cwd: '/repo',
      },
    ])
  })

  it('fails with the tail of the output and the log path when the build fails', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'e2e-build-'))
    try {
      const logFile = join(dir, 'build.log')
      const run: BuildRunner = async (_command, _args, options) => {
        const lines = Array.from({ length: 60 }, (_, i) => `line ${i + 1}`)
        writeFileSync(options.logFile, lines.join('\n') + '\n')
        return 1
      }
      await assert.rejects(
        buildDebugApplication({ repoRoot: '/repo', logFile, marker: 'm', run }),
        (error: unknown) => {
          assert.ok(error instanceof BuildError)
          assert.match(error.message, /line 60/)
          assert.match(error.message, /line 21/)
          assert.doesNotMatch(error.message, /line 20\b/)
          assert.match(
            error.message,
            new RegExp(logFile.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')),
          )
          return true
        },
      )
    } finally {
      rmSync(dir, { recursive: true, force: true })
    }
  })
})
