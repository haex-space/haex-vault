// Part of `pnpm check:settings` (spec 085, FR-007, FR-008, SC-004): the camera session of the link
// scanner gives the camera back on every way out, and names what went wrong. Driven by a fake
// camera and a fake clock, so nothing waits.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { createRequire } from 'node:module'

import {
  CameraError,
  cameraErrorKind,
  createCameraScan,
  type CameraDeps,
  type Frame,
} from '../src/lib/sync/cameraScan.ts'

function fakeCamera(
  options: {
    openError?: { name: string } | ((n: number) => { name: string } | null)
    texts?: (string | null)[]
    hasApi?: boolean
  } = {},
) {
  const log: string[] = []
  const track = { stop: () => log.push('track stopped') }
  const queue = [...(options.texts ?? [])]
  let ticks: (() => void) | null = null
  let opens = 0
  const deps: CameraDeps = {
    begin: async () => void log.push('gate open'),
    end: async () => void log.push('gate closed'),
    hasCameraApi: () => options.hasApi ?? true,
    getUserMedia: async () => {
      opens += 1
      const error =
        typeof options.openError === 'function'
          ? options.openError(opens)
          : options.openError
      if (error) throw error
      return { getTracks: () => [track] }
    },
    cameras: async () => [],
    grab: () => ({ data: new Uint8ClampedArray(4), width: 1, height: 1 }),
    decode: () => queue.shift() ?? null,
    every: (_ms, tick) => {
      ticks = tick
      return () => void (ticks = null)
    },
  }
  const video = { srcObject: null as unknown, play: async () => {} }
  return {
    deps,
    video,
    log,
    tick: () => ticks?.(),
    ticking: () => ticks !== null,
    opens: () => opens,
  }
}

// jsqr is CommonJS; the app imports it as a module through the bundler.
const jsQR = createRequire(import.meta.url)('jsqr') as (
  data: Uint8ClampedArray,
  width: number,
  height: number,
) => { data: string } | null

const settle = () => new Promise((resolve) => setImmediate(resolve))

test('a taken text releases the camera and closes the gate in the same go', async () => {
  const camera = fakeCamera({ texts: ['haex-vault://link?v=1'] })
  const scan = createCameraScan(camera.deps)
  const seen: string[] = []

  await scan.start(camera.video, async (text) => {
    seen.push(text)
    return true
  })
  assert.equal(scan.running, true)
  camera.tick()
  await settle()

  assert.deepEqual(seen, ['haex-vault://link?v=1'])
  assert.equal(scan.running, false)
  assert.equal(camera.ticking(), false)
  assert.equal(camera.video.srcObject, null)
  assert.deepEqual(camera.log, ['gate open', 'track stopped', 'gate closed'])
})

test('a text that is not taken keeps the camera running', async () => {
  const camera = fakeCamera({ texts: ['hello', 'haex-vault://link?v=1'] })
  const scan = createCameraScan(camera.deps)
  const seen: string[] = []

  await scan.start(camera.video, async (text) => {
    seen.push(text)
    return text.startsWith('haex-vault')
  })
  camera.tick()
  await settle()
  assert.equal(scan.running, true)
  camera.tick()
  await settle()

  assert.deepEqual(seen, ['hello', 'haex-vault://link?v=1'])
  assert.equal(scan.running, false)
})

test('later frames do not start a second read while one is answered', async () => {
  const camera = fakeCamera({ texts: ['a', 'b'] })
  const scan = createCameraScan(camera.deps)
  let release: (taken: boolean) => void = () => {}
  const seen: string[] = []

  await scan.start(
    camera.video,
    (text) =>
      new Promise((resolve) => {
        seen.push(text)
        release = resolve
      }),
  )
  camera.tick()
  camera.tick()
  await settle()
  assert.deepEqual(seen, ['a'])
  release(false)
  await settle()
  camera.tick()
  await settle()

  assert.deepEqual(seen, ['a', 'b'])
})

test('closing releases the camera, twice is fine', async () => {
  const camera = fakeCamera()
  const scan = createCameraScan(camera.deps)
  await scan.start(camera.video, async () => false)

  await scan.stop()
  await scan.stop()

  assert.equal(scan.running, false)
  assert.equal(camera.log.filter((l) => l === 'track stopped').length, 1)
  assert.equal(camera.log.at(-1), 'gate closed')
})

for (const [name, kind] of [
  ['NotFoundError', 'noCamera'],
  ['NotReadableError', 'busy'],
  ['NotAllowedError', 'denied'],
  ['SomethingElse', 'failed'],
] as const) {
  test(`${name} is reported as ${kind} and the gate is closed again`, async () => {
    const camera = fakeCamera({ openError: { name } })
    const scan = createCameraScan(camera.deps)

    await assert.rejects(
      scan.start(camera.video, async () => false),
      (error: unknown) => error instanceof CameraError && error.kind === kind,
    )

    assert.equal(scan.running, false)
    assert.equal(camera.log.at(-1), 'gate closed')
  })
}

test('no camera API at all is reported as unsupported', async () => {
  const camera = fakeCamera({ hasApi: false })
  const scan = createCameraScan(camera.deps)

  await assert.rejects(
    scan.start(camera.video, async () => false),
    (error: unknown) =>
      error instanceof CameraError && error.kind === 'unsupported',
  )
  assert.equal(camera.log.at(-1), 'gate closed')
})

test('a camera that cannot do the back camera is asked again without the wish', async () => {
  const camera = fakeCamera({
    openError: (n) => (n === 1 ? { name: 'OverconstrainedError' } : null),
  })
  const scan = createCameraScan(camera.deps)

  await scan.start(camera.video, async () => false)

  assert.equal(camera.opens(), 2)
  assert.equal(scan.running, true)
})

test('error names map to the cases a person can act on', () => {
  assert.equal(cameraErrorKind({ name: 'NotFoundError' }), 'noCamera')
  assert.equal(cameraErrorKind({ name: 'NotReadableError' }), 'busy')
  assert.equal(cameraErrorKind({ name: 'NotAllowedError' }), 'denied')
  assert.equal(cameraErrorKind(null), 'failed')
  assert.equal(cameraErrorKind(new Error('x')), 'failed')
})

// The decoder reads what the host draws (SC-002 groundwork): the QR codes of a usual link text and
// of one at the size limit, rendered from their module matrix as a camera would see them.
function image(file: string): Frame {
  const rows = readFileSync(
    new URL(`./fixtures/${file}`, import.meta.url),
    'utf8',
  )
    .split('\n')
    .filter((line) => line.length > 0)
  const modules = rows.map((row) => Array.from(row, (cell) => cell === '#'))
  const scale = 4
  const quiet = 4
  const size = (modules.length + 2 * quiet) * scale
  const data = new Uint8ClampedArray(size * size * 4).fill(255)
  modules.forEach((row, y) =>
    row.forEach((dark, x) => {
      if (!dark) return
      for (let dy = 0; dy < scale; dy++)
        for (let dx = 0; dx < scale; dx++) {
          const at =
            (((y + quiet) * scale + dy) * size + (x + quiet) * scale + dx) * 4
          data[at] = data[at + 1] = data[at + 2] = 0
        }
    }),
  )
  return { data, width: size, height: size }
}

test('the decoder reads a QR code of a usual link text', () => {
  const frame = image('link-qr-usual.txt')

  assert.equal(
    jsQR(frame.data, frame.width, frame.height)?.data,
    'haex-vault://link?v=1&c=ABCDEFGHIJKLMNOPQRSTUVWXYA&n=Privat&nr=wss%3A%2F%2Frelay.example.org&off=wss%3A%2F%2Fnos.lol',
  )
})

test('the decoder reads a QR code of a text at the size limit', () => {
  const frame = image('link-qr-large.txt')
  const text = readFileSync(
    new URL('./fixtures/link-qr-large.text', import.meta.url),
    'utf8',
  )

  assert.equal(text.length, 1200)
  assert.equal(jsQR(frame.data, frame.width, frame.height)?.data, text)
})

// The camera of a pass (spec 092, FR-034): the codes of a frame are read by the backend, so the
// decoder answers with a promise and the session looks at no further frame meanwhile.
type Codes = { format: string; value: string }[]

test('a decoder that answers later holds the next frames back, then delivers and releases', async () => {
  const camera = fakeCamera()
  let looked = 0
  let answer: (found: Codes | null) => void = () => {}
  const scan = createCameraScan<Codes>({
    ...camera.deps,
    decode: () => {
      looked += 1
      return new Promise((resolve) => (answer = resolve))
    },
  })
  const seen: Codes[] = []
  await scan.start(camera.video, async (found) => {
    seen.push(found)
    return true
  })
  camera.tick()
  camera.tick()
  camera.tick()
  assert.equal(looked, 1, 'a frame is looked at while the last one is read')

  answer(null)
  await settle()
  camera.tick()
  assert.equal(
    looked,
    2,
    'the next frame is looked at once the last one is read',
  )
  assert.equal(scan.running, true)

  answer([{ format: 'pdf417', value: 'T1' }])
  await settle()
  assert.deepEqual(seen, [[{ format: 'pdf417', value: 'T1' }]])
  assert.equal(scan.running, false)
  assert.deepEqual(camera.log, ['gate open', 'track stopped', 'gate closed'])
})

test('a decoder that rejects ends the scan with the camera given back', async () => {
  const camera = fakeCamera()
  const scan = createCameraScan<Codes>({
    ...camera.deps,
    decode: () => Promise.reject(new Error('backend gone')),
  })
  const errors: string[] = []
  await scan.start(
    camera.video,
    async () => true,
    undefined,
    (error) => errors.push(error.kind),
  )
  camera.tick()
  await settle()
  await settle()

  assert.deepEqual(errors, ['failed'])
  assert.equal(scan.running, false)
  assert.equal(camera.ticking(), false)
  assert.deepEqual(camera.log, ['gate open', 'track stopped', 'gate closed'])
})

test('a closed scan drops a late answer of the decoder', async () => {
  const camera = fakeCamera()
  let answer: (found: Codes | null) => void = () => {}
  const scan = createCameraScan<Codes>({
    ...camera.deps,
    decode: () => new Promise((resolve) => (answer = resolve)),
  })
  const seen: Codes[] = []
  await scan.start(camera.video, async (found) => {
    seen.push(found)
    return true
  })
  camera.tick()
  await scan.stop()
  answer([{ format: 'qr', value: 'late' }])
  await settle()

  assert.deepEqual(seen, [])
})
