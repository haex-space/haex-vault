import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { effectiveTimeoutMs, registerName, runScenario } from './scenario.ts'
import type { E2EEnv, RunDeps } from './scenario.ts'
import type { Instance } from './instance.ts'

function withRunDir<T>(fn: (runDir: string) => Promise<T>): Promise<T> {
  const runDir = mkdtempSync(join(tmpdir(), 'e2e-scenario-'))
  return fn(runDir).finally(() =>
    rmSync(runDir, { recursive: true, force: true }),
  )
}

function envFor(runDir: string, extra: Partial<E2EEnv> = {}): E2EEnv {
  return {
    runDir,
    app: '/app/holzi',
    closeBehavior: 'exit',
    tools: {
      tauriDriver: '/t/tauri-driver',
      webKitWebDriver: '/t/WebKitWebDriver',
      xvfbRun: '/t/xvfb-run',
    },
    scenarioTimeoutMs: 60_000,
    timeScale: 1,
    marker: 'test-marker',
    keep: false,
    ...extra,
  }
}

function depsFor(
  runDir: string,
  extra: Partial<RunDeps> = {},
  env: Partial<E2EEnv> = {},
): RunDeps {
  return {
    env: envFor(runDir, env),
    startInstance: async () =>
      ({ stop: async () => {} }) as unknown as Instance,
    ...extra,
  }
}

describe('runScenario', () => {
  it('skips a scenario that needs the other close behavior and never runs its body', () =>
    withRunDir(async (runDir) => {
      let ran = false
      const result = await runScenario(
        'needs-relaunch',
        { needs: { closeBehavior: 'relaunch' } },
        async () => {
          ran = true
        },
        depsFor(runDir),
      )
      assert.equal(ran, false)
      assert.equal(result.status, 'skipped')
      assert.equal(
        result.skipReason,
        'the build exits on close; this scenario needs one that relaunches',
      )

      const reverse = await runScenario(
        'needs-exit',
        { needs: { closeBehavior: 'exit' } },
        async () => {},
        depsFor(runDir, {}, { closeBehavior: 'relaunch' }),
      )
      assert.equal(reverse.status, 'skipped')
      assert.equal(
        reverse.skipReason,
        'the build relaunches on close; this scenario needs one that exits',
      )
    }))

  it('runs a scenario whose need matches the application', () =>
    withRunDir(async (runDir) => {
      const result = await runScenario(
        'matching-need',
        { needs: { closeBehavior: 'exit' } },
        async () => {},
        depsFor(runDir),
      )
      assert.equal(result.status, 'passed')
    }))

  it('fails a throwing body and runs every teardown once, in reverse order', () =>
    withRunDir(async (runDir) => {
      const order: string[] = []
      const result = await runScenario(
        'throws',
        {},
        async (ctx) => {
          ctx.onTeardown(async () => void order.push('a'))
          ctx.onTeardown(async () => void order.push('b'))
          throw new Error('the body broke')
        },
        depsFor(runDir),
      )
      assert.equal(result.status, 'failed')
      assert.match(result.error ?? '', /the body broke/)
      assert.deepEqual(order, ['b', 'a'])
    }))

  it('fails a body that reaches its deadline, tears down, and lets the next scenario run', () =>
    withRunDir(async (runDir) => {
      let tornDown = false
      const result = await runScenario(
        'hangs',
        { timeoutMs: 100 },
        async (ctx) => {
          ctx.onTeardown(async () => {
            tornDown = true
          })
          await new Promise(() => {})
        },
        depsFor(runDir),
      )
      assert.equal(result.status, 'failed')
      assert.match(result.error ?? '', /deadline of 100 ms/)
      assert.equal(tornDown, true)

      const next = await runScenario(
        'after-hang',
        {},
        async () => {},
        depsFor(runDir),
      )
      assert.equal(next.status, 'passed')
    }))

  it('marks a scenario failed when its teardown fails even though the body passed', () =>
    withRunDir(async (runDir) => {
      let seenFailure: string | undefined
      const result = await runScenario(
        'teardown-fails',
        {},
        async (ctx) => {
          ctx.step('ready')
          ctx.onTeardown(async () => {
            throw new Error('could not stop it')
          })
        },
        depsFor(runDir, {
          onFailure: async (info) => {
            seenFailure = `${info.error instanceof Error ? info.error.message : info.error}:${info.failedStep}`
          },
        }),
      )
      assert.equal(result.status, 'failed')
      assert.match(result.error ?? '', /teardown failed: could not stop it/)
      assert.equal(result.failedStep, 'ready')
      assert.equal(seenFailure, 'teardown failed: could not stop it:ready')
    }))

  it('names an instance that was still starting when a deadline was reached', () =>
    withRunDir(async (runDir) => {
      let seenFailure: string | undefined
      const result = await runScenario(
        'starting-instance',
        { timeoutMs: 20 },
        async (ctx) => {
          await ctx.startInstance()
        },
        depsFor(runDir, {
          startInstance: async () =>
            await new Promise<Instance>(() => {
              // Deliberately never resolves: this models a driver/app that never becomes ready.
            }),
          onFailure: async (info) => {
            seenFailure = info.failedStep
          },
        }),
      )
      assert.equal(result.status, 'failed')
      assert.equal(result.failedStep, 'instance-ready')
      assert.equal(seenFailure, 'instance-ready')
    }))

  it('calls the failure hook before teardown', () =>
    withRunDir(async (runDir) => {
      const order: string[] = []
      await runScenario(
        'hook-order',
        {},
        async (ctx) => {
          ctx.onTeardown(async () => void order.push('teardown'))
          throw new Error('x')
        },
        depsFor(runDir, {
          onFailure: async () => void order.push('failure-hook'),
        }),
      )
      assert.deepEqual(order, ['failure-hook', 'teardown'])
    }))

  it('passes the instances and providers started so far to the failure hook', () =>
    withRunDir(async (runDir) => {
      let seenProviders: unknown
      await runScenario(
        'hook-providers',
        {},
        async (ctx) => {
          await ctx.provider()
          throw new Error('x')
        },
        depsFor(runDir, {
          onFailure: async (info) => {
            seenProviders = info.providers
          },
        }),
      )
      assert.equal((seenProviders as unknown[]).length, 1)
    }))

  it('records the failed step from the last one reached, or from a timed-out wait', () =>
    withRunDir(async (runDir) => {
      const fromStep = await runScenario(
        'failed-after-step',
        {},
        async (ctx) => {
          ctx.step('unlocked')
          throw new Error('x')
        },
        depsFor(runDir),
      )
      assert.equal(fromStep.failedStep, 'unlocked')
      assert.equal(fromStep.material, join(runDir, 'failed-after-step'))

      const fromWait = await runScenario(
        'failed-in-wait',
        { timeoutMs: 10_000 },
        async (ctx) => {
          await ctx.waitFor('the reply to arrive', () => false, {
            timeoutMs: 30,
          })
        },
        depsFor(runDir),
      )
      assert.equal(fromWait.failedStep, 'the reply to arrive')
    }))

  it('removes the scenario directory after a pass, unless the run keeps it', () =>
    withRunDir(async (runDir) => {
      const material = join(runDir, 'kept-check')
      mkdirSync(material, { recursive: true })
      writeFileSync(join(material, 'driver.log'), 'hello')

      await runScenario('kept-check', {}, async () => {}, depsFor(runDir))
      assert.equal(existsSync(material), false)

      mkdirSync(material, { recursive: true })
      writeFileSync(join(material, 'driver.log'), 'hello')
      await runScenario(
        'kept-check',
        {},
        async () => {},
        depsFor(runDir, {}, { keep: true }),
      )
      assert.equal(existsSync(material), true)
    }))

  it('starts an instance through the context, records instance-ready and stops it in teardown', () =>
    withRunDir(async (runDir) => {
      const events: string[] = []
      const result = await runScenario(
        'starts-instance',
        {},
        async (ctx) => {
          await ctx.startInstance()
        },
        depsFor(runDir, {
          startInstance: async () => {
            events.push('start')
            return {
              stop: async () => void events.push('stop'),
            } as unknown as Instance
          },
        }),
      )
      assert.equal(result.status, 'passed')
      assert.deepEqual(events, ['start', 'stop'])
      assert.ok(result.steps.some((s) => s.name === 'instance-ready'))
    }))

  it('removes a failed startup root and preserves the original error and diagnostics', () =>
    withRunDir(async (runDir) => {
      const root = join(runDir, 'instances', 'failed-start-1')
      const logFile = join(runDir, 'failed-start', 'driver.log')
      const startupError = new Error('the application failed to start')
      let seenFailure: unknown
      const result = await runScenario(
        'failed-start',
        {},
        async (ctx) => {
          await ctx.startInstance()
        },
        depsFor(runDir, {
          startInstance: async (request) => {
            assert.equal(request.root, root)
            mkdirSync(join(root, 'home'), { recursive: true })
            mkdirSync(join(root, 'runtime'), { recursive: true })
            mkdirSync(join(runDir, 'failed-start'), { recursive: true })
            writeFileSync(request.logFile, 'startup diagnostics')
            throw startupError
          },
          onFailure: async (info) => {
            seenFailure = info.error
          },
        }),
      )
      assert.equal(result.status, 'failed')
      assert.equal(seenFailure, startupError)
      assert.equal(existsSync(root), false)
      assert.equal(readFileSync(logFile, 'utf8'), 'startup diagnostics')
    }))

  it('records steps from one monotonic clock', () =>
    withRunDir(async (runDir) => {
      const result = await runScenario(
        'steps',
        {},
        async (ctx) => {
          ctx.step('first')
          ctx.step('second', 'with detail')
          ctx.step('device-step', undefined, 'anna/laptop')
        },
        depsFor(runDir),
      )
      const [a, b, deviceStep] = result.steps
      assert.deepEqual([a.name, b.name], ['first', 'second'])
      assert.equal(b.detail, 'with detail')
      assert.equal(deviceStep?.device, 'anna/laptop')
      assert.ok(Number.isInteger(a.atMs) && a.atMs >= 0 && b.atMs >= a.atMs)
    }))

  it('writes the result to <run dir>/results/<name>.json', () =>
    withRunDir(async (runDir) => {
      await runScenario('written', {}, async () => {}, depsFor(runDir))
      const written = JSON.parse(
        readFileSync(join(runDir, 'results', 'written.json'), 'utf8'),
      )
      assert.equal(written.name, 'written')
      assert.equal(written.status, 'passed')
      assert.equal(typeof written.durationMs, 'number')
      assert.ok(Array.isArray(written.steps))
    }))

  it('starts a stand-in provider through the context and closes it in teardown', () =>
    withRunDir(async (runDir) => {
      let baseUrl = ''
      const result = await runScenario(
        'uses-provider',
        {},
        async (ctx) => {
          const provider = await ctx.provider()
          baseUrl = provider.baseUrl
          assert.match(baseUrl, /^http:\/\/127\.0\.0\.1:\d+$/)
          const response = await fetch(`${baseUrl}/v1/models`)
          assert.equal(response.status, 200)
        },
        depsFor(runDir),
      )
      assert.equal(result.status, 'passed')
      await assert.rejects(fetch(baseUrl))
    }))
})

describe('waitFor', () => {
  it('polls at most every 50 ms until the value is truthy', () =>
    withRunDir(async (runDir) => {
      let calls = 0
      const result = await runScenario(
        'polls',
        {},
        async (ctx) => {
          const value = await ctx.waitFor(
            'the third call',
            () => (++calls >= 3 ? 'done' : 0),
            { timeoutMs: 2000 },
          )
          assert.equal(value, 'done')
        },
        depsFor(runDir),
      )
      assert.equal(result.status, 'passed', result.error ?? '')
      assert.equal(calls, 3)
    }))

  it('names the description and the last observed value when it times out', () =>
    withRunDir(async (runDir) => {
      let calls = 0
      const result = await runScenario(
        'times-out',
        {},
        async (ctx) => {
          await ctx.waitFor('the widget to show', () => (calls++, 0), {
            timeoutMs: 300,
          })
        },
        depsFor(runDir),
      )
      assert.equal(result.status, 'failed')
      assert.match(result.error ?? '', /the widget to show/)
      assert.match(result.error ?? '', /last observed: 0/)
      assert.ok(calls >= 3, `polled ${calls} times`)
    }))

  const pageState = {
    href: 'http://localhost/files',
    title: 'T',
    readyState: 'complete',
    viewport: '1x1',
    focused: 'button[data-testid=files-new]',
    dialogs: ['div[data-testid=sync-rule]: Sync rule'],
    toasts: [],
    testIds: ['a', 'b'],
    text: '',
  }

  it('says what the wait saw and where the page stood, when it times out', () =>
    withRunDir(async (runDir) => {
      const result = await runScenario(
        'times-out-with-page',
        {},
        async (ctx) => {
          await ctx.startInstance()
          await ctx.waitFor('two tiles', () => false, {
            timeoutMs: 200,
            observe: () => ['a-one'],
          })
        },
        depsFor(runDir, {
          startInstance: async () =>
            ({
              stop: async () => {},
              alive: () => true,
              exec: async () => pageState,
            }) as unknown as Instance,
        }),
      )
      assert.equal(result.status, 'failed')
      assert.match(result.error ?? '', /last observed: false/)
      assert.match(result.error ?? '', /observed: \["a-one"\]/)
      assert.match(
        result.error ?? '',
        /page: address http:\/\/localhost\/files, focus button\[data-testid=files-new\], dialogs div\[data-testid=sync-rule\]: Sync rule, toasts none, 2 test ids/,
      )
    }))

  it('still reports the timeout when the page or the observer fails', () =>
    withRunDir(async (runDir) => {
      const result = await runScenario(
        'times-out-without-page',
        {},
        async (ctx) => {
          await ctx.startInstance()
          await ctx.waitFor('two tiles', () => false, {
            timeoutMs: 200,
            observe: () => {
              throw new Error('probe broke')
            },
          })
        },
        depsFor(runDir, {
          startInstance: async () =>
            ({
              stop: async () => {},
              alive: () => true,
              exec: async () => {
                throw new Error('session gone')
              },
            }) as unknown as Instance,
        }),
      )
      assert.equal(result.status, 'failed')
      assert.match(
        result.error ?? '',
        /timed out after \d+ ms waiting for two tiles/,
      )
      assert.match(
        result.error ?? '',
        /observed: not available \(probe broke\)/,
      )
      assert.match(result.error ?? '', /page: not available \(session gone\)/)
    }))
})

describe('waitFor diagnostics stay inside their limits', () => {
  const page = (href: string) => ({
    href,
    title: 'T',
    readyState: 'complete',
    viewport: '1x1',
    focused: 'nothing',
    dialogs: [],
    toasts: [],
    testIds: [],
    text: '',
  })
  const instance = (href: string) =>
    ({
      stop: async () => {},
      alive: () => true,
      exec: async () => page(href),
    }) as unknown as Instance

  it('names every live instance, each with its own label', () =>
    withRunDir(async (runDir) => {
      let started = 0
      const result = await runScenario(
        'two-instances',
        {},
        async (ctx) => {
          await ctx.startInstance()
          await ctx.startInstance()
          await ctx.waitFor('nothing', () => false, { timeoutMs: 100 })
        },
        depsFor(runDir, {
          startInstance: async () => instance(`http://localhost/${++started}`),
        }),
      )
      assert.match(
        result.error ?? '',
        /instance 1 page: address http:\/\/localhost\/1/,
      )
      assert.match(
        result.error ?? '',
        /instance 2 page: address http:\/\/localhost\/2/,
      )
    }))

  it('does not wait for an observer that never answers', () =>
    withRunDir(async (runDir) => {
      const result = await runScenario(
        'silent-observer',
        {},
        async (ctx) => {
          await ctx.waitFor('nothing', () => false, {
            timeoutMs: 100,
            observe: () => new Promise(() => {}),
          })
        },
        depsFor(runDir),
      )
      assert.match(
        result.error ?? '',
        /timed out after 100 ms waiting for nothing/,
      )
      assert.match(
        result.error ?? '',
        /observed: not available \(no answer within \d+ ms\)/,
      )
    }))

  it('keeps the wait message when the scenario deadline is near', () =>
    withRunDir(async (runDir) => {
      const result = await runScenario(
        'near-deadline',
        { timeoutMs: 400 },
        async (ctx) => {
          await ctx.startInstance()
          await ctx.waitFor('nothing', () => false, { timeoutMs: 100 })
        },
        depsFor(runDir, {
          startInstance: async () =>
            ({
              stop: async () => {},
              alive: () => true,
              exec: () => new Promise(() => {}),
            }) as unknown as Instance,
        }),
      )
      assert.match(
        result.error ?? '',
        /timed out after 100 ms waiting for nothing/,
      )
      assert.ok(
        !/reached its deadline/.test(result.error ?? ''),
        result.error ?? 'no error',
      )
    }))
})

describe('timeouts and names', () => {
  it('defaults to the run limit and multiplies generic timeouts by the time scale', () => {
    const env = envFor('/run')
    assert.equal(effectiveTimeoutMs({}, env), 60_000)
    assert.equal(effectiveTimeoutMs({ timeoutMs: 5000 }, env), 5000)
    const scaled = envFor('/run', { timeScale: 2 })
    assert.equal(effectiveTimeoutMs({}, scaled), 120_000)
    assert.equal(effectiveTimeoutMs({ timeoutMs: 5000 }, scaled), 10_000)
  })

  it('rejects a scenario name registered twice', () => {
    registerName('unique-name-for-the-test')
    assert.throws(
      () => registerName('unique-name-for-the-test'),
      /duplicate scenario name/,
    )
  })
})
