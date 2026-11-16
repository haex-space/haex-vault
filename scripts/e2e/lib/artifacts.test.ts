import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { captureFailure } from './artifacts.ts'
import type { CaptureDevice } from './group.ts'
import type { Instance } from './instance.ts'
import type { Provider } from './provider.ts'

function withRunDir<T>(fn: (runDir: string) => Promise<T>): Promise<T> {
  const runDir = mkdtempSync(join(tmpdir(), 'e2e-artifacts-'))
  return fn(runDir).finally(() =>
    rmSync(runDir, { recursive: true, force: true }),
  )
}

function fakeInstance(overrides: Partial<Instance> = {}): Instance {
  return {
    alive: () => true,
    screenshot: async () => Buffer.from('png'),
    ...overrides,
  } as Instance
}

function fakeProvider(overrides: Partial<Provider> = {}): Provider {
  return {
    connections: () => [],
    requests: () => [],
    ...overrides,
  } as Provider
}

describe('captureFailure', () => {
  it('writes timeline.json with the steps, the failure message and the deadline reached', () =>
    withRunDir(async (runDir) => {
      await captureFailure({
        runDir,
        scenario: 'a',
        error: new Error('boom'),
        failedStep: 'instance-ready',
        steps: [
          { name: 'instance-ready', atMs: 10, at: '2026-01-01T00:00:00.000Z' },
        ],
        deadlineMs: 4000,
        instances: [],
        providers: [],
      })
      const timeline = JSON.parse(
        readFileSync(join(runDir, 'a', 'timeline.json'), 'utf8'),
      )
      assert.deepEqual(timeline.steps, [
        { name: 'instance-ready', atMs: 10, at: '2026-01-01T00:00:00.000Z' },
      ])
      assert.equal(timeline.error, 'boom')
      assert.equal(timeline.failedStep, 'instance-ready')
      assert.equal(timeline.deadlineMs, 4000)
    }))

  it('takes a screenshot of the first instance', () =>
    withRunDir(async (runDir) => {
      await captureFailure({
        runDir,
        scenario: 'a',
        error: new Error('x'),
        steps: [],
        instances: [fakeInstance()],
        providers: [],
      })
      assert.equal(
        readFileSync(join(runDir, 'a', 'screenshot.png'), 'utf8'),
        'png',
      )
    }))

  it('leaves out the screenshot and notes it in the timeline when the application had already ended', () =>
    withRunDir(async (runDir) => {
      await captureFailure({
        runDir,
        scenario: 'a',
        error: new Error('x'),
        steps: [],
        instances: [fakeInstance({ alive: () => false })],
        providers: [],
      })
      assert.equal(existsSync(join(runDir, 'a', 'screenshot.png')), false)
      const timeline = JSON.parse(
        readFileSync(join(runDir, 'a', 'timeline.json'), 'utf8'),
      )
      assert.match(timeline.screenshotNote, /already ended/)
    }))

  it('never throws when the session is gone, and still writes the other files', () =>
    withRunDir(async (runDir) => {
      await captureFailure({
        runDir,
        scenario: 'a',
        error: new Error('x'),
        steps: [],
        instances: [
          fakeInstance({
            screenshot: async () => {
              throw new Error('no such window')
            },
          }),
        ],
        providers: [],
      })
      assert.equal(existsSync(join(runDir, 'a', 'screenshot.png')), false)
      assert.ok(existsSync(join(runDir, 'a', 'timeline.json')))
      assert.ok(existsSync(join(runDir, 'a', 'provider.json')))
    }))

  it('writes provider.json with each provider’s connections and requests', () =>
    withRunDir(async (runDir) => {
      await captureFailure({
        runDir,
        scenario: 'a',
        error: new Error('x'),
        steps: [],
        instances: [],
        providers: [
          fakeProvider({
            connections: () => [
              { id: 1, openedAt: 0, method: 'POST', path: '/v1/messages' },
            ],
            requests: () => [
              { id: 1, at: 0, method: 'GET', path: '/v1/models' },
            ],
          }),
        ],
      })
      const provider = JSON.parse(
        readFileSync(join(runDir, 'a', 'provider.json'), 'utf8'),
      )
      assert.deepEqual(provider, [
        {
          connections: [
            { id: 1, openedAt: 0, method: 'POST', path: '/v1/messages' },
          ],
          requests: [{ id: 1, at: 0, method: 'GET', path: '/v1/models' }],
        },
      ])
    }))

  it('never throws even if the run directory itself cannot be created', async () => {
    await withRunDir(async (runDir) => {
      const file = join(runDir, 'not-a-directory')
      writeFileSync(file, '')
      await captureFailure({
        runDir: file,
        scenario: 'a',
        error: new Error('x'),
        steps: [],
        instances: [],
        providers: [],
      })
    })
  })
})

describe('captureFailure with several devices', () => {
  function device(
    folder: string,
    overrides: Partial<CaptureDevice> = {},
  ): CaptureDevice {
    return {
      folder,
      alive: () => true,
      screenshot: async () => new Uint8Array([1, 2, 3]),
      keepData: (into) => writeFileSync(join(into, 'data.txt'), folder),
      ...overrides,
    }
  }

  const base = {
    scenario: 'sc',
    error: new Error('boom'),
    steps: [],
    instances: [],
    providers: [],
  }

  it('gives each device a folder with its screenshot and its data', () =>
    withRunDir(async (runDir) => {
      await captureFailure({
        ...base,
        runDir,
        devices: [device('4-anna-6-laptop'), device('3-ben-2-pc')],
      })
      for (const folder of ['4-anna-6-laptop', '3-ben-2-pc']) {
        assert.ok(existsSync(join(runDir, 'sc', folder, 'screenshot.png')))
        assert.equal(
          readFileSync(join(runDir, 'sc', folder, 'data.txt'), 'utf8'),
          folder,
        )
      }
      const timeline = JSON.parse(
        readFileSync(join(runDir, 'sc', 'timeline.json'), 'utf8'),
      )
      assert.deepEqual(timeline.devices, [
        {
          folder: '4-anna-6-laptop',
          files: ['data.txt', 'page-state.txt', 'screenshot.png'],
        },
        {
          folder: '3-ben-2-pc',
          files: ['data.txt', 'page-state.txt', 'screenshot.png'],
        },
      ])
    }))

  it('leaves a note for a device that was not running', () =>
    withRunDir(async (runDir) => {
      await captureFailure({
        ...base,
        runDir,
        devices: [device('d', { alive: () => false })],
      })
      assert.match(
        readFileSync(join(runDir, 'sc', 'd', 'screenshot-note.txt'), 'utf8'),
        /not running/,
      )
      assert.equal(existsSync(join(runDir, 'sc', 'd', 'screenshot.png')), false)
    }))

  it('notes a failed screenshot and a failed data copy and still handles the next device', () =>
    withRunDir(async (runDir) => {
      await captureFailure({
        ...base,
        runDir,
        devices: [
          device('a', {
            screenshot: async () => {
              throw new Error('the session is gone')
            },
            keepData: () => {
              throw new Error('disk full')
            },
          }),
          device('b'),
        ],
      })
      assert.match(
        readFileSync(join(runDir, 'sc', 'a', 'screenshot-note.txt'), 'utf8'),
        /the session is gone/,
      )
      assert.match(
        readFileSync(join(runDir, 'sc', 'a', 'data-note.txt'), 'utf8'),
        /disk full/,
      )
      assert.ok(existsSync(join(runDir, 'sc', 'b', 'screenshot.png')))
    }))

  it('writes no device section for a scenario without a group', () =>
    withRunDir(async (runDir) => {
      await captureFailure({ ...base, runDir })
      const timeline = JSON.parse(
        readFileSync(join(runDir, 'sc', 'timeline.json'), 'utf8'),
      )
      assert.equal('devices' in timeline, false)
    }))
})

describe('captureFailure: what the page saw', () => {
  const state = {
    href: 'http://localhost/files',
    title: 'HaexVault',
    readyState: 'complete',
    viewport: '800x600',
    focused: 'button[data-testid=files-new]',
    dialogs: ['div[data-testid=sync-rule]: Sync rule'],
    toasts: [],
    testIds: ['files-new', 'files-list'],
    text: 'Files New',
  }

  it('writes page-state.txt from the page of the first instance', () =>
    withRunDir(async (runDir) => {
      await captureFailure({
        runDir,
        scenario: 'a',
        error: new Error('x'),
        steps: [],
        instances: [fakeInstance({ exec: async () => state as never })],
        providers: [],
      })
      const text = readFileSync(join(runDir, 'a', 'page-state.txt'), 'utf8')
      assert.match(text, /address:\s+http:\/\/localhost\/files/)
      assert.match(text, /focused:\s+button\[data-testid=files-new\]/)
      assert.match(text, /- div\[data-testid=sync-rule\]: Sync rule/)
      assert.match(text, /files-new, files-list/)
    }))

  it('says why there is no page state instead of leaving nothing', () =>
    withRunDir(async (runDir) => {
      await captureFailure({
        runDir,
        scenario: 'a',
        error: new Error('x'),
        steps: [],
        instances: [
          fakeInstance({
            exec: async () => {
              throw new Error('session gone')
            },
          }),
        ],
        providers: [],
      })
      assert.equal(
        readFileSync(join(runDir, 'a', 'page-state.txt'), 'utf8'),
        'no page state: session gone\n',
      )
    }))

  it('names an ended application', () =>
    withRunDir(async (runDir) => {
      await captureFailure({
        runDir,
        scenario: 'a',
        error: new Error('x'),
        steps: [],
        instances: [fakeInstance({ alive: () => false })],
        providers: [],
      })
      assert.match(
        readFileSync(join(runDir, 'a', 'page-state.txt'), 'utf8'),
        /already ended/,
      )
    }))

  it('keeps the log of a phone scenario next to the page state', () =>
    withRunDir(async (runDir) => {
      await captureFailure({
        runDir,
        scenario: 'a',
        error: new Error('x'),
        steps: [],
        instances: [
          fakeInstance({
            exec: async () => state as never,
            pid: 4242,
            phone: { logTail: (pid: number) => `log of ${pid}` } as never,
          }),
        ],
        providers: [],
      })
      assert.equal(
        readFileSync(join(runDir, 'a', 'logcat.txt'), 'utf8'),
        'log of 4242',
      )
    }))

  it('says why the log is missing, and a desktop instance has none', () =>
    withRunDir(async (runDir) => {
      await captureFailure({
        runDir,
        scenario: 'a',
        error: new Error('x'),
        steps: [],
        instances: [
          fakeInstance({
            exec: async () => state as never,
            phone: {
              logTail: () => {
                throw new Error('adb offline')
              },
            } as never,
          }),
        ],
        providers: [],
      })
      assert.equal(
        readFileSync(join(runDir, 'a', 'logcat.txt'), 'utf8'),
        'no log: adb offline\n',
      )
      await captureFailure({
        runDir,
        scenario: 'b',
        error: new Error('x'),
        steps: [],
        instances: [fakeInstance({ exec: async () => state as never })],
        providers: [],
      })
      assert.equal(existsSync(join(runDir, 'b', 'logcat.txt')), false)
    }))
})

describe('captureFailure: the page of a group device', () => {
  const device = (overrides: Partial<CaptureDevice> = {}): CaptureDevice => ({
    folder: 'alice-1',
    alive: () => true,
    screenshot: async () => new Uint8Array(),
    keepData: () => {},
    ...overrides,
  })
  const run = (runDir: string, devices: CaptureDevice[]) =>
    captureFailure({
      runDir,
      scenario: 'g',
      error: new Error('x'),
      steps: [],
      instances: [],
      providers: [],
      devices,
    })
  const read = (runDir: string) =>
    readFileSync(join(runDir, 'g', 'alice-1', 'page-state.txt'), 'utf8')

  it('writes the page of a device that can say, and why it cannot otherwise', () =>
    withRunDir(async (runDir) => {
      await run(runDir, [
        device({
          pageState: async () => ({
            href: 'http://localhost/chat',
            title: 'T',
            readyState: 'complete',
            viewport: '1x1',
            focused: 'nothing',
            dialogs: [],
            toasts: [],
            testIds: [],
            text: '',
          }),
        }),
      ])
      assert.match(read(runDir), /address:\s+http:\/\/localhost\/chat/)
      await run(runDir, [device()])
      assert.equal(read(runDir), 'no page state: the device cannot be asked\n')
      await run(runDir, [device({ alive: () => false })])
      assert.equal(read(runDir), 'no page state: the device was not running\n')
      await run(runDir, [
        device({
          pageState: async () => {
            throw new Error('page silent')
          },
        }),
      ])
      assert.equal(read(runDir), 'no page state: page silent\n')
    }))
})
