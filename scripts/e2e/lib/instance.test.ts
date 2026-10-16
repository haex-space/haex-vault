import { afterEach, beforeEach, describe, it } from 'node:test'
import assert from 'node:assert/strict'
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  appInGroup,
  buildInstanceEnv,
  driverCommand,
  isPortInUseOutput,
  NEW_SESSION_ATTEMPTS,
  newSessionWithRetry,
  prepareRoot,
  removeRoot,
  removeRootSettled,
} from './instance.ts'
import { WebDriverClient } from './webdriver.ts'
import { startFakeDriver } from './fake-driver.testlib.ts'
import type { FakeDriver } from './fake-driver.testlib.ts'

function scratch(): string {
  return mkdtempSync(join(tmpdir(), 'e2e-instance-'))
}

describe('buildInstanceEnv', () => {
  const root = '/scratch/instance-1'
  const parent = {
    PATH: '/usr/bin',
    HOME: '/sentinel/home',
    XDG_DATA_HOME: '/sentinel/data',
    XDG_CONFIG_HOME: '/sentinel/config',
    XDG_CACHE_HOME: '/sentinel/cache',
    XDG_RUNTIME_DIR: '/sentinel/run',
    XDG_STATE_HOME: '/sentinel/state',
    XDG_DATA_DIRS: '/nix/share',
    DBUS_SESSION_BUS_ADDRESS: 'unix:path=/sentinel/bus',
    WAYLAND_DISPLAY: 'wayland-9',
    DISPLAY: ':9',
  }

  it('puts every location the application writes to inside the instance root', () => {
    const env = buildInstanceEnv(root, 'the-marker', parent)
    assert.equal(env.XDG_DATA_HOME, `${root}/data`)
    assert.equal(env.XDG_CONFIG_HOME, `${root}/config`)
    assert.equal(env.XDG_CACHE_HOME, `${root}/cache`)
    assert.equal(env.XDG_RUNTIME_DIR, `${root}/run`)
    assert.equal(env.HOME, `${root}/home`)
  })

  it('cuts the application off from the desktop session and marks the run', () => {
    const env = buildInstanceEnv(root, 'the-marker', parent)
    assert.equal(env.DBUS_SESSION_BUS_ADDRESS, 'disabled:')
    assert.equal(env.HOLZI_E2E_RUN, 'the-marker')
    assert.equal(env.GDK_BACKEND, 'x11')
    assert.equal(env.WAYLAND_DISPLAY, undefined)
    assert.equal(env.DISPLAY, undefined)
    assert.equal(env.XDG_STATE_HOME, undefined)
  })

  it('keeps what the application needs to run and leaks nothing of the parent', () => {
    const env = buildInstanceEnv(root, 'the-marker', parent)
    assert.equal(env.PATH, '/usr/bin')
    assert.equal(env.XDG_DATA_DIRS, '/nix/share')
    for (const [key, value] of Object.entries(env)) {
      assert.ok(
        !String(value).includes('/sentinel'),
        `${key} leaks the parent's value`,
      )
    }
  })
})

describe('prepareRoot and removeRoot', () => {
  it('creates the tree empty, with a private runtime directory, and a light scheme by default', () => {
    const parent = scratch()
    try {
      const root = join(parent, 'root')
      prepareRoot(root)
      for (const dir of ['data', 'cache', 'run', 'home']) {
        assert.deepEqual(
          readdirSync(join(root, dir)),
          [],
          `${dir} starts empty`,
        )
      }
      assert.deepEqual(readdirSync(join(root, 'config')), ['gtk-3.0'])
      assert.equal(statSync(join(root, 'run')).mode & 0o777, 0o700)
      const ini = readFileSync(
        join(root, 'config', 'gtk-3.0', 'settings.ini'),
        'utf8',
      )
      assert.match(ini, /gtk-application-prefer-dark-theme=false/)
    } finally {
      rmSync(parent, { recursive: true, force: true })
    }
  })

  it('writes the dark preference for the dark scheme', () => {
    const parent = scratch()
    try {
      const root = join(parent, 'root')
      prepareRoot(root, { colorScheme: 'dark' })
      const ini = readFileSync(
        join(root, 'config', 'gtk-3.0', 'settings.ini'),
        'utf8',
      )
      assert.match(ini, /gtk-application-prefer-dark-theme=true/)
    } finally {
      rmSync(parent, { recursive: true, force: true })
    }
  })

  it('refuses a root that already holds something, unless it is being reused', () => {
    const parent = scratch()
    try {
      const root = join(parent, 'root')
      mkdirSync(join(root, 'data'), { recursive: true })
      writeFileSync(join(root, 'data', 'vault.db'), 'x')
      assert.throws(() => prepareRoot(root), /not empty/)
      prepareRoot(root, { reuse: true })
      assert.equal(readFileSync(join(root, 'data', 'vault.db'), 'utf8'), 'x')
    } finally {
      rmSync(parent, { recursive: true, force: true })
    }
  })

  it('removes the whole tree', () => {
    const parent = scratch()
    try {
      const root = join(parent, 'root')
      prepareRoot(root)
      removeRoot(root)
      assert.throws(() => statSync(root), /ENOENT/)
    } finally {
      rmSync(parent, { recursive: true, force: true })
    }
  })
})

describe('appInGroup', () => {
  const groups = new Map([
    [100, 10],
    [200, 20],
    [300, 20],
  ])
  const groupOf = (pid: number) => groups.get(pid) ?? null

  it('picks the application of the instance whose driver leads the group, not the first one found', () => {
    const candidates = [{ pid: 100 }, { pid: 200 }]
    assert.equal(appInGroup(candidates, 20, groupOf), 200)
    assert.equal(appInGroup(candidates, 10, groupOf), 100)
  })

  it('finds nothing when no candidate is in the group', () => {
    assert.equal(appInGroup([{ pid: 100 }], 99, groupOf), undefined)
    assert.equal(appInGroup([], 10, groupOf), undefined)
  })

  it('never picks the runner itself', () => {
    const groupOfAll = () => 20
    assert.equal(appInGroup([{ pid: process.pid }], 20, groupOfAll), undefined)
  })
})

describe('driverCommand', () => {
  it('starts the driver on its own virtual screen with the given ports and tools', () => {
    const tools = {
      tauriDriver: '/t/tauri-driver',
      webKitWebDriver: '/t/WebKitWebDriver',
      xvfbRun: '/t/xvfb-run',
    }
    assert.deepEqual(driverCommand(tools, { driver: 1111, native: 2222 }), {
      command: '/t/xvfb-run',
      args: [
        '-a',
        '-s',
        '-screen 0 1280x800x24',
        '/t/tauri-driver',
        '--port',
        '1111',
        '--native-port',
        '2222',
        '--native-driver',
        '/t/WebKitWebDriver',
      ],
    })
  })

  it("adds -fbdir inside the screen's own server-args string, not as a separate xvfb-run argument", () => {
    const tools = {
      tauriDriver: '/t/tauri-driver',
      webKitWebDriver: '/t/WebKitWebDriver',
      xvfbRun: '/t/xvfb-run',
    }
    const { args } = driverCommand(
      tools,
      { driver: 1111, native: 2222 },
      { framebufferDir: '/scratch/fb' },
    )
    assert.equal(args[1], '-s')
    assert.equal(args[2], '-screen 0 1280x800x24 -fbdir /scratch/fb')
  })
})

describe('isPortInUseOutput', () => {
  it("recognizes WebKitWebDriver's listen failure", () => {
    assert.equal(
      isPortInUseOutput(
        'FATAL: Unable to listen for HTTP server at host 127.0.0.1 and port 44749.',
      ),
      true,
    )
  })

  it('does not classify an unrelated driver failure as a port collision', () => {
    assert.equal(isPortInUseOutput('FATAL: failed to initialize WebKit'), false)
  })
})

describe('newSessionWithRetry', () => {
  let driver: FakeDriver
  let client: WebDriverClient

  beforeEach(async () => {
    driver = await startFakeDriver()
    client = new WebDriverClient(driver.url)
  })

  afterEach(() => driver.close())

  it('succeeds at once when the first attempt does', async () => {
    await newSessionWithRetry(client, '/some/app')
    assert.equal(client.sessionId, driver.sessionId)
  })

  it('retries once when the driver answers before its native connection is ready', async () => {
    let calls = 0
    driver.onNewSession(() => {
      calls += 1
      return calls === 1 ? 'drop' : 'ok'
    })
    await newSessionWithRetry(client, '/some/app')
    assert.equal(calls, 2)
    assert.equal(client.sessionId, driver.sessionId)
  })

  it('retries a few times and then gives up with the last failure', async () => {
    let calls = 0
    driver.onNewSession(() => {
      calls += 1
      return 'drop'
    })
    await assert.rejects(newSessionWithRetry(client, '/some/app'))
    assert.equal(calls, NEW_SESSION_ATTEMPTS)
  })

  it('succeeds when a later attempt does', async () => {
    let calls = 0
    driver.onNewSession(() => {
      calls += 1
      return calls < 3 ? 'drop' : 'ok'
    })
    await newSessionWithRetry(client, '/some/app')
    assert.equal(calls, 3)
  })

  it('does not retry a definitive session creation failure', async () => {
    let calls = 0
    driver.onNewSession(() => {
      calls += 1
      return 'not-created'
    })
    await assert.rejects(
      newSessionWithRetry(client, '/some/app'),
      /session not created/,
    )
    assert.equal(calls, 1)
  })
})

describe('removeRootSettled', () => {
  const notEmpty = () => {
    throw Object.assign(new Error('ENOTEMPTY'), { code: 'ENOTEMPTY' })
  }

  it('stops what runs in the root and tries again until it is removed', async () => {
    let removals = 0
    const stopped: string[] = []
    await removeRootSettled('/root', {
      remove: () => {
        if (++removals < 3) notEmpty()
      },
      stopLeftovers: (root) => {
        stopped.push(root)
        return Promise.resolve([])
      },
      pauseMs: 0,
    })
    assert.equal(removals, 3)
    assert.deepEqual(stopped, ['/root', '/root'])
  })

  it('does not stop anything when the first removal works', async () => {
    let stops = 0
    await removeRootSettled('/root', {
      remove: () => {},
      stopLeftovers: () => {
        stops++
        return Promise.resolve([])
      },
    })
    assert.equal(stops, 0)
  })

  it('gives the error of the last attempt when the root stays', async () => {
    let removals = 0
    await assert.rejects(
      removeRootSettled('/root', {
        remove: () => {
          removals++
          notEmpty()
        },
        stopLeftovers: () => Promise.resolve([]),
        attempts: 3,
        pauseMs: 0,
      }),
      /ENOTEMPTY/,
    )
    assert.equal(removals, 3)
  })
})
