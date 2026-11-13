// Checks the pure parts of capturing a code for a pass (spec 092, FR-034).
// Run: node --test scripts/check-wallet-capture.ts
import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  MAX_PAGE_PIXELS,
  MAX_PAGE_SIDE,
  PAGES_PER_STEP,
  addFound,
  formatFromPlugin,
  newPassCodes,
  pageScale,
  pageStep,
  withPicked,
} from '../src/lib/wallet/capture.ts'

test('a short PDF is read in one step', () => {
  assert.deepEqual(pageStep(3, 0), { pages: [1, 2, 3], more: false })
  assert.deepEqual(pageStep(0, 0), { pages: [], more: false })
})

test('a long PDF is read in steps of twenty pages', () => {
  const first = pageStep(45, 0)
  assert.equal(first.pages.length, PAGES_PER_STEP)
  assert.equal(first.pages[0], 1)
  assert.equal(first.pages.at(-1), 20)
  assert.equal(first.more, true)
  const last = pageStep(45, 40)
  assert.deepEqual(last.pages, [41, 42, 43, 44, 45])
  assert.equal(last.more, false)
  // Asking past the end gives nothing, and a negative start is the start.
  assert.deepEqual(pageStep(45, 99), { pages: [], more: false })
  assert.equal(pageStep(45, -5).pages[0], 1)
})

test('a code found twice is one code', () => {
  const a = { format: 'qr', value: 'A' }
  const b = { format: 'qr', value: 'B' }
  assert.deepEqual(addFound([a], [a, b, a]), [a, b])
  // The same text in another format is another code.
  assert.equal(addFound([a], [{ format: 'aztec', value: 'A' }]).length, 2)
})

test('picked codes join the codes a pass has without touching them', () => {
  const existing = [
    { id: 'c1', format: 'qr', value: 'ONE', label: 'Entry', position: 0 },
    { id: 'c2', format: 'code128', value: 'TWO', position: 4 },
  ]
  const merged = withPicked(existing, [
    { format: 'qr', value: 'ONE' },
    { format: 'aztec', value: 'THREE' },
  ])
  assert.deepEqual(
    merged.map((c) => [c.id, c.format, c.value, c.position]),
    [
      ['c1', 'qr', 'ONE', 0],
      ['c2', 'code128', 'TWO', 4],
      [undefined, 'aztec', 'THREE', 5],
    ],
  )
  // The input is not changed.
  assert.equal(existing.length, 2)
})

test('a new pass gets the picked codes in order', () => {
  const codes = newPassCodes([
    { format: 'qr', value: 'A' },
    { format: 'ean13', value: '4006381333931' },
  ])
  assert.deepEqual(
    codes.map((c) => [c.format, c.position]),
    [
      ['qr', 0],
      ['ean13', 1],
    ],
  )
})

test('a page is drawn at the capture width, within the pixel bound', () => {
  // A letter page: the capture width decides.
  const letter = pageScale(612, 792)
  assert.ok(letter !== null && Math.abs(letter * 612 - 1800) < 1e-6)
  // A tiny page is not blown up more than four times.
  assert.equal(pageScale(10, 10), 4)
  // A page declared very tall stays within the pixel bound.
  const tall = pageScale(600, 10_000_000)
  assert.ok(
    tall !== null && 600 * tall * (10_000_000 * tall) <= MAX_PAGE_PIXELS + 1,
  )
  assert.ok(tall !== null && 10_000_000 * tall <= MAX_PAGE_SIDE + 1e-6)
  const huge = pageScale(1e7, 1e7)
  assert.ok(huge !== null && 1e7 * huge * (1e7 * huge) <= MAX_PAGE_PIXELS + 1)
})

test('a page without a usable size is skipped', () => {
  for (const [w, h] of [
    [0, 100],
    [100, 0],
    [-1, 100],
    [Number.NaN, 100],
    [100, Number.POSITIVE_INFINITY],
  ]) {
    assert.equal(pageScale(w, h), null, `${w}x${h}`)
  }
})

test('the formats of the barcode plugin map to the stored formats of a pass', () => {
  assert.equal(formatFromPlugin('QR_CODE'), 'qr')
  assert.equal(formatFromPlugin('CODE_128'), 'code128')
  assert.equal(formatFromPlugin('EAN_13'), 'ean13')
  assert.equal(formatFromPlugin('PDF_417'), 'pdf417')
  assert.equal(formatFromPlugin('AZTEC'), 'aztec')
})

test('a format a pass does not keep maps to nothing', () => {
  for (const format of ['UPC_A', 'DATA_MATRIX', 'CODE_39', '', 'qr']) {
    assert.equal(formatFromPlugin(format), null, format)
  }
})
