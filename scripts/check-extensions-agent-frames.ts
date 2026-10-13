// Part of `pnpm check:extensions`: which tab has an extension frame whose port is ready, so Rust can
// open its tool connection there (src/lib/extensions/agentFrames.ts, spec 018, research R9).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { ReadyFrames } from '../src/lib/extensions/agentFrames.ts'

test('a ready frame is answered at once', async () => {
  const frames = new ReadyFrames()
  frames.ready('tab-1', 'frame-a')
  assert.equal(await frames.waitFor('tab-1', 1000), 'frame-a')
})

test('a waiting request gets the frame once its port is ready', async () => {
  const frames = new ReadyFrames()
  const waiting = frames.waitFor('tab-1', 1000)
  frames.ready('tab-2', 'frame-other')
  frames.ready('tab-1', 'frame-a')
  assert.equal(await waiting, 'frame-a')
})

test('no ready frame within the limit answers null', async () => {
  const frames = new ReadyFrames()
  assert.equal(await frames.waitFor('tab-1', 20), null)
})

test('a frame that is gone or reloading is not ready until its new page is', async () => {
  const frames = new ReadyFrames()
  frames.ready('tab-1', 'frame-a')
  frames.gone('tab-1')
  assert.equal(frames.frameOf('tab-1'), null)
  const waiting = frames.waitFor('tab-1', 1000)
  frames.ready('tab-1', 'frame-b')
  assert.equal(await waiting, 'frame-b')
})
