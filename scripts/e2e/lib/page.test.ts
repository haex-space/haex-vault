import { after, before, describe, it } from 'node:test'
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import type { ChildProcess } from 'node:child_process'
import { readlinkSync } from 'node:fs'
import {
  click,
  createPage,
  waitUntilGone,
  markedProcesses,
  press,
  sampleUntilEnd,
  toSelector,
  type,
  waitForEnd,
} from './page.ts'
import { WebDriverClient } from './webdriver.ts'
import { startFakeDriver } from './fake-driver.testlib.ts'
import type { FakeDriver } from './fake-driver.testlib.ts'
import { newMarker } from './processes.ts'

const spawned: ChildProcess[] = []

/** A long sleep that carries the given marker, or none (mirrors processes.test.ts's `sleeper`). */
function sleeper(marker?: string): ChildProcess & { pid: number } {
  const env = { ...process.env }
  delete env.HOLZI_E2E_RUN
  if (marker) env.HOLZI_E2E_RUN = marker
  const child = spawn('sleep', ['60'], { env, stdio: 'ignore' })
  spawned.push(child)
  assert.ok(child.pid !== undefined)
  return child as ChildProcess & { pid: number }
}

let driver: FakeDriver
let client: WebDriverClient

before(async () => {
  driver = await startFakeDriver()
  client = new WebDriverClient(driver.url)
  await client.newSession('/some/app')
})

after(async () => {
  await driver.close()
  for (const child of spawned) {
    try {
      child.kill('SIGKILL')
    } catch {
      // already gone
    }
  }
})

describe('toSelector', () => {
  it('uses a value starting with #, . or [ as a selector as is, and wraps anything else as a data-testid', () => {
    assert.equal(toSelector('#unlock-passphrase'), '#unlock-passphrase')
    assert.equal(toSelector('.ring'), '.ring')
    assert.equal(toSelector('[form="unlock-form"]'), '[form="unlock-form"]')
    assert.equal(toSelector('lock-instance'), '[data-testid="lock-instance"]')
  })
})

describe('click', () => {
  it('polls until is-displayed reports true for a found element, then clicks it', async () => {
    driver.onFind(() => ['el-1'])
    let checks = 0
    driver.onDisplayed(() => {
      checks += 1
      return checks >= 3
    })
    await click(client, 'open-chat', 200)
    assert.equal(checks, 3)
    assert.ok(
      driver.requests.some(
        (r) => r.method === 'POST' && r.path.endsWith('/element/el-1/click'),
      ),
    )
    assert.ok(
      driver.requests.some(
        (r) =>
          r.method === 'POST' &&
          r.path.endsWith('/execute/async') &&
          (r.body as { script: string }).script.includes('scrollIntoView'),
      ),
    )
  })

  it('clicks the displayed one when the hook finds more than one element', async () => {
    driver.onFind(() => ['el-1', 'el-2'])
    driver.onDisplayed((id) => id === 'el-2')
    const before = driver.requests.length
    await click(client, 'lock-instance')
    const made = driver.requests.slice(before)
    assert.ok(
      made.some(
        (r) => r.method === 'POST' && r.path.endsWith('/element/el-2/click'),
      ),
    )
    assert.ok(
      !made.some(
        (r) => r.method === 'POST' && r.path.endsWith('/element/el-1/click'),
      ),
    )
  })

  it('retries a transient element-not-interactable response', async () => {
    driver.onFind(() => ['el-1'])
    driver.onDisplayed(() => true)
    let attempts = 0
    driver.onClick(() => {
      attempts += 1
      return attempts === 1 ? 'not-interactable' : 'ok'
    })
    await click(client, 'open-chat')
    assert.equal(attempts, 2)
  })

  it('retries a click that a moving window intercepted, and gives up at the deadline', async () => {
    driver.onFind(() => ['el-1'])
    driver.onDisplayed(() => true)
    let attempts = 0
    driver.onClick(() => {
      attempts += 1
      return attempts <= 2 ? 'intercepted' : 'ok'
    })
    await click(client, 'chat-new')
    assert.equal(attempts, 3)
    driver.onClick(() => 'intercepted')
    await assert.rejects(click(client, 'chat-new', 120), /click intercepted/)
  })

  it('finds the control again when it went stale between finding and clicking', async () => {
    let finds = 0
    driver.onFind(() => {
      finds += 1
      return [`el-${finds}`]
    })
    driver.onDisplayed(() => true)
    const clicked: string[] = []
    driver.onClick((element) => {
      clicked.push(element)
      return clicked.length === 1 ? 'stale' : 'ok'
    })
    await click(client, 'passwords-crumb-root')
    assert.deepEqual(clicked, ['el-1', 'el-2'])
  })

  it('fails naming the hook and the selector when nothing is displayed by the deadline', async () => {
    driver.onFind(() => ['el-1'])
    driver.onDisplayed(() => false)
    await assert.rejects(
      click(client, 'open-chat', 80),
      /hook "open-chat" \(selector \[data-testid="open-chat"\]\) was not displayed within \d+ ms/,
    )
  })
})

describe('type', () => {
  it('sends the text to the displayed element found by hook', async () => {
    driver.onFind((_using, value) => {
      assert.equal(value, '#unlock-passphrase')
      return ['el-1']
    })
    driver.onDisplayed(() => true)
    await type(client, '#unlock-passphrase', 'sekret')
    const typed = driver.requests
      .filter((r) => r.path.endsWith('/element/el-1/value'))
      .pop()
    assert.deepEqual(typed?.body, { text: 'sekret' })
  })
})

describe('press', () => {
  it('clicks once and records the step, without polling', async () => {
    driver.onFind(() => ['el-1'])
    driver.onDisplayed(() => true)
    driver.onClick(() => 'ok')
    const steps: Array<[string, string | undefined]> = []
    const before = driver.requests.length
    await press(client, 'lock-instance', {
      step: (name, detail) => steps.push([name, detail]),
    })
    const clicks = driver.requests
      .slice(before)
      .filter((r) => r.path.endsWith('/click'))
    assert.equal(clicks.length, 1)
    assert.deepEqual(steps, [['press', 'lock-instance']])
  })

  it('resolves with the time the click was sent, after the element lookups before it', async () => {
    driver.onFind(() => ['el-1'])
    let lastLookupAt = 0
    driver.onDisplayed(() => {
      const until = Date.now() + 30 // a slow lookup, as under load on CI
      while (Date.now() < until) {
        // spin
      }
      lastLookupAt = Date.now()
      return true
    })
    let clickedAt = 0
    driver.onClick(() => {
      clickedAt = Date.now()
      return 'ok'
    })
    const pressedAt = await press(client, 'lock-instance', { step: () => {} })
    assert.ok(
      pressedAt >= lastLookupAt,
      `pressedAt ${pressedAt} before lookup ${lastLookupAt}`,
    )
    assert.ok(
      pressedAt <= clickedAt,
      `pressedAt ${pressedAt} after click ${clickedAt}`,
    )
  })

  it('with times: 2 clicks the same element twice', async () => {
    driver.onFind(() => ['el-1'])
    driver.onDisplayed(() => true)
    driver.onClick(() => 'ok')
    const before = driver.requests.length
    await press(client, 'lock-instance', { times: 2, step: () => {} })
    const clicks = driver.requests
      .slice(before)
      .filter((r) => r.path.endsWith('/click'))
    assert.equal(clicks.length, 2)
  })

  it('uses the element activated by the first click for later activations', async () => {
    let finds = 0
    driver.onFind(() => {
      finds += 1
      return [finds === 1 ? 'el-old' : 'el-new']
    })
    driver.onDisplayed(() => true)
    driver.onClick(() => 'ok')
    const before = driver.requests.length
    await press(client, 'lock-instance', { times: 2, step: () => {} })
    const clicks = driver.requests
      .slice(before)
      .filter((r) => r.path.endsWith('/click'))
      .map((r) => r.path)
    assert.deepEqual(clicks, [
      '/session/fake-session/element/el-new/click',
      '/session/fake-session/element/el-new/click',
    ])
  })

  it('stops quietly when a later click finds the session already gone', async () => {
    driver.onFind(() => ['el-1'])
    driver.onDisplayed(() => true)
    let calls = 0
    driver.onClick(() => {
      calls += 1
      return calls === 1 ? 'ok' : 'drop'
    })
    const steps: Array<[string, string | undefined]> = []
    await press(client, 'lock-instance', {
      times: 3,
      step: (name, detail) => steps.push([name, detail]),
    })
    assert.equal(calls, 2) // the third click is never sent once the second finds the session gone
    assert.deepEqual(steps, [['press', 'lock-instance']])
  })

  it('stops quietly when the first click races the process ending', async () => {
    driver.onFind(() => ['el-1'])
    driver.onDisplayed(() => true)
    driver.onClick(() => 'drop')
    const steps: Array<[string, string | undefined]> = []
    await press(client, 'lock-instance', {
      step: (name, detail) => steps.push([name, detail]),
    })
    assert.deepEqual(steps, [['press', 'lock-instance']])
  })

  it('also stops quietly on a stale element reference (seen under load: the DOM moved on before the session was gone)', async () => {
    driver.onFind(() => ['el-1'])
    driver.onDisplayed(() => true)
    let calls = 0
    driver.onClick(() => {
      calls += 1
      return calls === 1 ? 'ok' : 'stale'
    })
    const steps: Array<[string, string | undefined]> = []
    await press(client, 'lock-instance', {
      times: 2,
      step: (name, detail) => steps.push([name, detail]),
    })
    assert.equal(calls, 2)
    assert.deepEqual(steps, [['press', 'lock-instance']])
  })

  it('also stops quietly when the ending process makes the element non-interactable', async () => {
    driver.onFind(() => ['el-1'])
    driver.onDisplayed(() => true)
    let calls = 0
    driver.onClick(() => {
      calls += 1
      return calls === 1 ? 'ok' : 'not-interactable'
    })
    await press(client, 'lock-instance', { times: 2, step: () => {} })
    assert.equal(calls, 2)
  })

  it('still throws if the very first click fails for another reason', async () => {
    driver.onFind(() => ['el-1'])
    driver.onDisplayed(() => true)
    driver.onClick(() => 'invalid')
    await assert.rejects(
      press(client, 'lock-instance', { times: 2, step: () => {} }),
      /invalid argument/,
    )
  })

  it('fails at once, without polling, when the control is not displayed', async () => {
    driver.onFind(() => ['el-1'])
    let checks = 0
    driver.onDisplayed(() => {
      checks += 1
      return false
    })
    await assert.rejects(
      press(client, 'lock-instance', { step: () => {} }),
      /hook "lock-instance" \(selector \[data-testid="lock-instance"\]\) is not displayed/,
    )
    assert.equal(checks, 1)
  })
})

describe(
  'waitForEnd',
  { skip: process.platform === 'linux' ? false : 'needs /proc (Linux only)' },
  () => {
    it('resolves with the elapsed time and records process-ended once the process is gone', async () => {
      const child = spawn('sh', ['-c', 'sleep 0.1'])
      spawned.push(child)
      assert.ok(child.pid !== undefined)
      const steps: Array<[string, string | undefined]> = []
      const elapsed = await waitForEnd(child.pid, 2000, (n, d) =>
        steps.push([n, d]),
      )
      assert.ok(elapsed >= 0)
      assert.equal(steps[0]?.[0], 'process-ended')
    })

    it('fails at the deadline for a process that keeps running', async () => {
      const child = sleeper()
      await assert.rejects(
        waitForEnd(child.pid, 100, () => {}),
        /did not end within 100 ms/,
      )
    })
  },
)

describe(
  'markedProcesses',
  { skip: process.platform === 'linux' ? false : 'needs /proc (Linux only)' },
  () => {
    it('finds only the marked processes and only those running the given executable', () => {
      const marker = newMarker()
      const a = sleeper(marker)
      const b = sleeper(marker)
      sleeper() // unmarked, must not be found
      const exe = readlinkSync(`/proc/${a.pid}/exe`)
      const found = markedProcesses(marker, exe)
      assert.deepEqual(found.map((p) => p.pid).sort(), [a.pid, b.pid].sort())
      assert.deepEqual(markedProcesses(marker, '/nonexistent/binary'), [])
    })
  },
)

describe('sampleUntilEnd', () => {
  it('collects every sample until the session ends, and stops there', async () => {
    let n = 0
    driver.onExecute(() => {
      n += 1
      return n <= 3 ? { value: n } : 'drop'
    })
    const samples = await sampleUntilEnd<number>(client, 'return 1', 1)
    assert.deepEqual(samples, [1, 2, 3])
  })

  it('still throws a real script error instead of treating it as the end', async () => {
    driver.onExecute(() => ({
      status: 400,
      body: { value: { error: 'javascript error', message: 'boom' } },
    }))
    await assert.rejects(
      sampleUntilEnd(client, 'return 1', 1),
      /javascript error/,
    )
  })
})

describe('waitUntilGone', () => {
  it('resolves once alive() turns false and names the label at the deadline', async () => {
    let checks = 0
    const steps: string[] = []
    const elapsed = await waitUntilGone(
      () => ++checks < 3,
      'the app',
      2000,
      (n) => steps.push(n),
    )
    assert.ok(elapsed >= 0)
    assert.deepEqual(steps, ['process-ended'])
    await assert.rejects(
      waitUntilGone(
        () => true,
        'the app',
        100,
        () => {},
      ),
      /the app did not end within 100 ms/,
    )
  })
})

describe('createPage', () => {
  it('binds every helper to the given client, pid, marker and executable', async () => {
    driver.onExecute(() => ({ value: { ok: true, data: 'hi' } }))
    const page = createPage({
      client,
      appPid: process.pid,
      marker: 'unused:0:unused',
      executable: '/bin/true',
      step: () => {},
    })
    assert.deepEqual(await page.invoke('list_instances'), {
      ok: true,
      data: 'hi',
    })
    await page.navigate('tauri://localhost/closing.html')
    const nav = driver.requests.filter((r) => r.path.endsWith('/url')).pop()
    assert.deepEqual(nav?.body, { url: 'tauri://localhost/closing.html' })
  })

  it('asks the process functions and URL mapping a platform gives', async () => {
    let alive = true
    const page = createPage({
      client,
      step: () => {},
      process: {
        alive: () => alive,
        marked: () => [{ pid: 42, marker: 'm', exe: null }],
      },
      mapUrl: (url) =>
        url.replace('tauri://localhost', 'http://tauri.localhost'),
    })
    assert.deepEqual(page.markedProcesses(), [
      { pid: 42, marker: 'm', exe: null },
    ])
    await page.navigate('tauri://localhost/closing.html')
    const nav = driver.requests.filter((r) => r.path.endsWith('/url')).pop()
    assert.deepEqual(nav?.body, { url: 'http://tauri.localhost/closing.html' })
    alive = false
    assert.ok((await page.waitForEnd(500)) >= 0)
  })
})
