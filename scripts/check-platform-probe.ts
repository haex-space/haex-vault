import assert from 'node:assert/strict'
import { test } from 'node:test'

import { runPlatformProbe } from '../src/lib/platformProbe.ts'
import { judgeProbe, parseExpectation } from './lib/platformProbeResult.ts'

const answering = (status: number, body: string): typeof fetch =>
  (async () => new Response(body, { status })) as typeof fetch

test('a healthy loopback passes both steps', async () => {
  const report = await runPlatformProbe(4000, answering(200, 'ok'), 'UA')
  assert.deepEqual(report, {
    steps: [
      { name: 'webview', ok: true },
      { name: 'loopback-fetch', ok: true, detail: undefined },
    ],
    userAgent: 'UA',
  })
})

test('the probe fetches its own port on 127.0.0.1', async () => {
  let asked = ''
  const recording = (async (url: string | URL | Request) => {
    asked = String(url)
    return new Response('ok')
  }) as typeof fetch
  await runPlatformProbe(41873, recording, 'UA')
  assert.equal(asked, 'http://127.0.0.1:41873/__probe')
})

test('a blocked fetch fails the step with its reason', async () => {
  const blocked = (async () => {
    throw new TypeError('Failed to fetch')
  }) as typeof fetch
  const report = await runPlatformProbe(4000, blocked, 'UA')
  assert.deepEqual(report.steps[1], {
    name: 'loopback-fetch',
    ok: false,
    detail: 'TypeError: Failed to fetch',
  })
})

test('the context and policy violations explain a blocked fetch', async () => {
  const blocked = (async () => {
    throw new TypeError('Failed to fetch')
  }) as typeof fetch
  const report = await runPlatformProbe(4000, blocked, 'UA', {
    origin: 'http://tauri.localhost',
    secure: false,
    violations: () => ['connect-src http://127.0.0.1:4000/__probe'],
  })
  assert.deepEqual(report.steps[1], {
    name: 'context',
    ok: true,
    detail: 'origin http://tauri.localhost, secure context false',
  })
  assert.equal(
    report.steps[2]?.detail,
    'TypeError: Failed to fetch; policy violations: connect-src http://127.0.0.1:4000/__probe',
  )
})

test('a wrong answer fails the step', async () => {
  const report = await runPlatformProbe(4000, answering(404, ''), 'UA')
  assert.equal(report.steps[1]?.ok, false)
  assert.equal(report.steps[1]?.detail, 'status 404, body ""')
})

test('the CI judges the last result line', () => {
  const output = [
    'some log',
    'HOLZI_PROBE_RESULT {"ok":false}',
    '[INFO] HOLZI_PROBE_RESULT {"ok":true,"steps":[]}',
  ].join('\n')
  assert.deepEqual(judgeProbe(output, 0), {
    ok: true,
    message: 'platform probe passed',
  })
})

test('the CI fails without a result line, on ok false and on a non-zero exit', () => {
  assert.equal(judgeProbe('nothing here', 0).ok, false)
  assert.equal(judgeProbe('HOLZI_PROBE_RESULT {"ok":false}', 1).ok, false)
  assert.equal(judgeProbe('HOLZI_PROBE_RESULT {"ok":true}', 2).ok, false)
  assert.equal(judgeProbe('HOLZI_PROBE_RESULT not json', 0).ok, false)
})

test('logcat lines without an exit code can pass', () => {
  assert.equal(
    judgeProbe('10-08 I holzi: HOLZI_PROBE_RESULT {"ok":true}', null).ok,
    true,
  )
})

test('an expected step must be there with its detail', () => {
  const line = (detail: string) =>
    `HOLZI_PROBE_RESULT ${JSON.stringify({ ok: true, steps: [{ name: 'all-files', ok: true, detail }] })}`
  const expect = parseExpectation('all-files=granted: false')
  assert.deepEqual(expect, { step: 'all-files', detail: 'granted: false' })
  assert.equal(judgeProbe(line('granted: false'), null, expect).ok, true)
  const granted = judgeProbe(line('granted: true, 3 entries'), null, expect)
  assert.equal(granted.ok, false)
  assert.match(granted.message, /expected "granted: false"/)
  const missing = judgeProbe(
    'HOLZI_PROBE_RESULT {"ok":true,"steps":[]}',
    null,
    expect,
  )
  assert.equal(missing.ok, false)
})

test('an expectation needs a step name before the equals sign', () => {
  assert.equal(parseExpectation('=granted'), undefined)
  assert.equal(parseExpectation('all-files'), undefined)
  assert.equal(parseExpectation(undefined), undefined)
})
