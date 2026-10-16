import { after, before, describe, it } from 'node:test'
import assert from 'node:assert/strict'
import { WebDriverClient } from './webdriver.ts'
import { startFakeDriver } from './fake-driver.testlib.ts'
import type { FakeDriver } from './fake-driver.testlib.ts'

let driver: FakeDriver
let client: WebDriverClient

before(async () => {
  driver = await startFakeDriver()
  client = new WebDriverClient(driver.url)
})

after(async () => {
  await driver.close()
})

describe('WebDriverClient', () => {
  it('opens a session for the application and raises the script timeout to 60000', async () => {
    const id = await client.newSession('/some/app')
    assert.equal(id, driver.sessionId)
    assert.deepEqual(driver.requests[0].body, {
      capabilities: {
        alwaysMatch: { 'tauri:options': { application: '/some/app' } },
      },
    })
    const timeouts = driver.requests.find((r) => r.path.endsWith('/timeouts'))
    assert.deepEqual(timeouts?.body, { script: 60000 })
  })

  it('opens a session with the capabilities a platform gives instead of an application path', async () => {
    const other = new WebDriverClient(driver.url)
    const before = driver.requests.length
    await other.newSession({
      'goog:chromeOptions': { androidPackage: 'com.haex.holzi' },
    })
    assert.deepEqual(driver.requests[before]?.body, {
      capabilities: {
        alwaysMatch: {
          'goog:chromeOptions': { androidPackage: 'com.haex.holzi' },
        },
      },
    })
  })

  it('reads an element id from the W3C element key and clicks and types by id', async () => {
    driver.onFind((using, value) => (value === '#two' ? ['a', 'b'] : ['only']))
    assert.deepEqual(await client.findElements('css selector', '#two'), [
      'a',
      'b',
    ])
    assert.deepEqual(await client.findElements('css selector', '#one'), [
      'only',
    ])
    await client.click('a')
    await client.sendKeys('a', 'secret')
    const typed = driver.requests
      .filter((r) => r.path.endsWith('/element/a/value'))
      .pop()
    assert.deepEqual(typed?.body, { text: 'secret' })
    assert.ok(
      driver.requests.some(
        (r) => r.method === 'POST' && r.path.endsWith('/element/a/click'),
      ),
    )
  })

  it('reads whether an element is displayed', async () => {
    driver.onDisplayed((id) => id === 'shown')
    assert.equal(await client.isDisplayed('shown'), true)
    assert.equal(await client.isDisplayed('hidden'), false)
    assert.ok(
      driver.requests.some(
        (r) =>
          r.method === 'GET' && r.path.endsWith('/element/shown/displayed'),
      ),
    )
  })

  it('sends script, window, navigation and screenshot calls', async () => {
    driver.onExecute((kind) =>
      kind === 'sync'
        ? { value: 'result' }
        : { value: { ok: true, data: null } },
    )
    assert.equal(await client.execute('return 1', []), 'result')
    await client.navigate('tauri://localhost/closing.html')
    await client.closeWindow()
    const png = await client.screenshot()
    assert.ok(Buffer.isBuffer(png) && png.length > 0)
    const nav = driver.requests.filter((r) => r.path.endsWith('/url')).pop()
    assert.deepEqual(nav?.body, { url: 'tauri://localhost/closing.html' })
    assert.ok(
      driver.requests.some(
        (r) => r.method === 'DELETE' && r.path.endsWith('/window'),
      ),
    )
  })

  it('awaits a Promise through the asynchronous script endpoint', async () => {
    driver.onExecute((kind, script, args) => {
      assert.equal(kind, 'async')
      assert.match(script, /Promise\.resolve/)
      assert.deepEqual(args, ['value'])
      return { value: { ok: true, value: 'result' } }
    })
    assert.equal(
      await client.executeAsync('return Promise.resolve(arguments[0])', [
        'value',
      ]),
      'result',
    )
  })

  it('calls a backend command through the page and resolves the outcome instead of throwing', async () => {
    driver.onExecute(() => ({ value: { ok: true, data: 42 } }))
    assert.deepEqual(await client.invoke('list_instances', { a: 1 }), {
      ok: true,
      data: 42,
    })
    const call = driver.requests
      .filter((r) => r.path.endsWith('/execute/async'))
      .pop()
    const script = (call?.body as { script: string }).script
    assert.match(
      script,
      /window\.__TAURI_INTERNALS__\.invoke\("list_instances", \{"a":1\}\)/,
    )

    driver.onExecute(() => ({ value: { ok: false, error: 'boom' } }))
    assert.deepEqual(await client.invoke('list_instances'), {
      ok: false,
      error: 'boom',
    })
  })

  it('reports an ended application when the call gets no answer and the caller expected it', async () => {
    driver.onExecute(() => 'drop')
    assert.deepEqual(
      await client.invoke('close_instance', {}, { expectEnd: true }),
      { ended: true },
    )
    driver.onExecute(() => ({
      status: 500,
      body: { value: { error: 'unknown error', message: 'gone' } },
    }))
    assert.deepEqual(
      await client.invoke('close_instance', {}, { expectEnd: true }),
      { ended: true },
    )
  })

  it('reports an ended application when chromedriver lost the app it was attached to', async () => {
    driver.onExecute(() => ({
      status: 500,
      body: {
        value: {
          error: 'disconnected',
          message: 'disconnected: not connected to DevTools',
        },
      },
    }))
    assert.deepEqual(
      await client.invoke('close_instance', {}, { expectEnd: true }),
      { ended: true },
    )
  })

  it('throws an error that names the command when nothing was expected to end', async () => {
    driver.onExecute(() => 'drop')
    await assert.rejects(client.invoke('close_instance'), /close_instance/)
  })

  it('does not treat a script error as an ended application', async () => {
    driver.onExecute(() => ({
      status: 400,
      body: {
        value: { error: 'javascript error', message: 'x is not defined' },
      },
    }))
    await assert.rejects(
      client.invoke('list_instances', {}, { expectEnd: true }),
      /javascript error/,
    )
  })

  it('deletes the session', async () => {
    await client.deleteSession()
    assert.ok(
      driver.requests.some(
        (r) =>
          r.method === 'DELETE' && r.path === `/session/${driver.sessionId}`,
      ),
    )
    assert.equal(client.sessionId, null)
  })
})
