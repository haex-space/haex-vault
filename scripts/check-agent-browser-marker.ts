// Part of `pnpm check:browser-bridge`: what the marker of an agent tab counts as the user's input
// (spec 065 FR-017, `src-tauri/src/agent_browser/marker.js`). The script runs as it is, in a bare
// context standing for an embedded frame (the main frame builds a bar on top, the input rule is the
// same), with a clock and events the test drives.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { createContext, runInContext } from 'node:vm'

const source = readFileSync(
  new URL('../src-tauri/src/agent_browser/marker.js', import.meta.url),
  'utf8',
)

type Listener = (event: Record<string, unknown>) => void

function frame(start = 1000) {
  const listeners = new Map<string, Listener[]>()
  const reported: string[] = []
  let now = start
  const window: Record<string, unknown> = {
    top: {},
    addEventListener(type: string, listener: Listener) {
      listeners.set(type, [...(listeners.get(type) ?? []), listener])
    },
  }
  const context = createContext({
    window,
    performance: { now: () => now },
    __holzi: (what: string) => reported.push(what),
  })
  // Rust wraps the declaration in parentheses and calls it with the config.
  runInContext(`(${source})({})`, context)
  return {
    reported,
    advance: (ms: number) => (now += ms),
    focus: (target: unknown = window) =>
      listeners.get('focus')?.forEach((l) => l({ target })),
    press: (type: 'pointerdown' | 'keydown', extra = {}) =>
      listeners
        .get(type)
        ?.forEach((l) => l({ type, isTrusted: true, ...extra })),
    untrusted: () =>
      listeners
        .get('pointerdown')
        ?.forEach((l) => l({ type: 'pointerdown', isTrusted: false })),
    window,
  }
}

test('the click that focuses the window is not the user working in the page', () => {
  const f = frame()
  f.focus()
  f.advance(50)
  f.press('pointerdown')
  assert.deepEqual(f.reported, [])
})

test('a second press right after the focus click counts', () => {
  const f = frame()
  f.focus()
  f.advance(50)
  f.press('pointerdown')
  f.advance(50)
  f.press('pointerdown')
  assert.deepEqual(f.reported, ['input'])
})

test('a press later than the focus click window counts', () => {
  const f = frame()
  f.focus()
  f.advance(401)
  f.press('pointerdown')
  assert.deepEqual(f.reported, ['input'])
})

test('a press before the window has ever had the focus counts, even at the start of the document', () => {
  const f = frame(100)
  f.advance(10)
  f.press('pointerdown')
  assert.deepEqual(f.reported, ['input'])
})

test('the focus of an element in the page is no focus click', () => {
  const f = frame()
  f.focus({})
  f.press('pointerdown')
  assert.deepEqual(f.reported, ['input'])
})

test('a key counts, a modifier alone does not, an untrusted event never', () => {
  const f = frame()
  f.focus()
  f.press('keydown', { key: 'Alt' })
  f.press('keydown', { key: 'Super' })
  f.untrusted()
  assert.deepEqual(f.reported, [])
  f.press('keydown', { key: 'a' })
  assert.deepEqual(f.reported, ['input'])
})
