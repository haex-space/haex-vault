// Part of `pnpm check:passwords` (spec 092-wallet-passes, FR-008): the controller of the screen
// while a code is shown large (src/lib/passwords/screenBright.ts). A close that overtakes a
// pending enable leaves the screen given back and no wake lock held.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  createScreenController,
  newScreenShared,
  type Lock,
  type ScreenShared,
} from '../src/lib/passwords/screenBright.ts'

/** A promise that is settled by hand, so a test decides when a platform call answers. */
function gate<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

function fixture(visible = true, shared?: ScreenShared) {
  const state = {
    visible,
    native: [] as boolean[],
    held: 0,
    released: 0,
    pending: [] as ((lock: Lock | null) => void)[],
    lock: undefined as Lock | undefined,
  }
  const controller = createScreenController(
    {
      setNativeAsync: async (on) => {
        state.native.push(on)
      },
      requestLockAsync: () => {
        const answer = gate<Lock | null>()
        state.pending.push(answer.resolve)
        return answer.promise
      },
      isVisible: () => state.visible,
    },
    shared,
  )
  const lock = (): Lock => {
    state.held += 1
    return {
      release: async () => {
        state.held -= 1
        state.released += 1
      },
      addEventListener: () => undefined,
    }
  }
  return { state, controller, lock }
}

const tick = () => new Promise((done) => setTimeout(done, 0))

test('on takes the screen and the wake lock, off gives both back', async () => {
  const { state, controller, lock } = fixture()
  const on = controller.onAsync()
  await tick()
  state.pending[0]!(lock())
  await on
  assert.deepEqual(state.native, [true])
  assert.equal(state.held, 1)
  await controller.offAsync()
  assert.deepEqual(state.native, [true, false])
  assert.equal(state.held, 0)
})

test('a wake lock that arrives after the close is released at once', async () => {
  const { state, controller, lock } = fixture()
  const on = controller.onAsync()
  await tick()
  // The close comes while the request is still open.
  const off = controller.offAsync()
  state.pending[0]!(lock())
  await Promise.all([on, off])
  assert.equal(state.held, 0, 'no lock is left held')
  assert.equal(state.native.at(-1), false, 'the screen ends given back')
})

test('an enable that a close has overtaken does not enable the screen', async () => {
  const { state, controller } = fixture()
  // Two changes are queued before the first one runs.
  const on = controller.onAsync()
  const off = controller.offAsync()
  await tick()
  state.pending[0]?.(null)
  await Promise.all([on, off])
  assert.equal(state.native.at(-1), false)
  assert.equal(controller.wanted, false)
})

test('a hidden window gives the screen back and a visible one takes it again', async () => {
  const { state, controller, lock } = fixture()
  const on = controller.onAsync()
  await tick()
  state.pending[0]!(lock())
  await on
  state.visible = false
  await controller.visibilityChangedAsync()
  assert.equal(state.held, 0)
  assert.equal(state.native.at(-1), false)
  state.visible = true
  const back = controller.visibilityChangedAsync()
  await tick()
  state.pending[1]!(lock())
  await back
  assert.equal(state.held, 1)
  assert.equal(state.native.at(-1), true)
})

test('a visibility change while nothing is wanted does nothing', async () => {
  const { state, controller } = fixture()
  await controller.visibilityChangedAsync()
  assert.deepEqual(state.native, [])
})

test('a platform without a wake lock still takes the screen and throws nothing', async () => {
  const { state, controller } = fixture()
  const on = controller.onAsync()
  await tick()
  state.pending[0]!(null)
  await on
  assert.deepEqual(state.native, [true])
  await controller.offAsync()
  assert.deepEqual(state.native, [true, false])
})

test('a view that closes leaves the screen on while another view still wants it', async () => {
  const shared = newScreenShared()
  const a = fixture(true, shared)
  const b = fixture(true, shared)
  // View A closes while view B has just opened: B's enable is queued before A's close.
  const aOn = a.controller.onAsync()
  await tick()
  a.state.pending[0]!(a.lock())
  await aOn
  const bOn = b.controller.onAsync()
  const aOff = a.controller.offAsync()
  await tick()
  b.state.pending[0]!(b.lock())
  await Promise.all([bOn, aOff])
  assert.equal(a.state.native.at(-1), true, 'the screen stays on for B')
  assert.equal(b.state.native.at(-1), true)
  assert.equal(a.state.held, 0, 'A gave its own wake lock back')
  assert.equal(b.state.held, 1, 'B keeps its wake lock')
  await b.controller.offAsync()
  assert.equal(
    b.state.native.at(-1),
    false,
    'the last view gives the screen back',
  )
  assert.equal(b.state.held, 0)
})
