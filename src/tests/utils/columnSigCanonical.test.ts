import { describe, it, expect } from 'vitest'
import {
  affinityOf,
  toCanonicalBytes,
  toCanonicalBase64,
  bytesToBase64Standard,
} from '~/utils/columnSigCanonical'

/**
 * These tests lock down cross-language byte identity with
 * `src-tauri/src/crdt/column_sig/value_bytes.rs`. Runde 8 §I adds a shared
 * fixture that exercises both sides against the SAME expected outputs; the
 * vectors here mirror the Rust unit tests (`value_bytes_tests.rs`) so a
 * refactor on either side breaks fast, not silently at sig-verify time.
 */

/**
 * Normalise a Uint8Array to a plain number[] for structural comparison.
 * Vitest's `toEqual` on typed arrays occasionally reports false negatives
 * for buffers with different `byteOffset` / backing (e.g. the empty array
 * returned by `TextEncoder.encode('')`); comparing plain arrays is stable.
 */
const asArray = (u: Uint8Array): number[] => Array.from(u)

describe('affinityOf — SQLite type-affinity rules', () => {
  it('INT-containing declarations -> INTEGER', () => {
    expect(affinityOf('INTEGER')).toBe('INTEGER')
    expect(affinityOf('INT')).toBe('INTEGER')
    expect(affinityOf('BIGINT')).toBe('INTEGER')
    expect(affinityOf('TINYINT')).toBe('INTEGER')
  })

  it('TEXT-family declarations -> TEXT', () => {
    expect(affinityOf('TEXT')).toBe('TEXT')
    expect(affinityOf('VARCHAR(255)')).toBe('TEXT')
    expect(affinityOf('CLOB')).toBe('TEXT')
  })

  it('BLOB and empty declaration -> BLOB', () => {
    expect(affinityOf('BLOB')).toBe('BLOB')
    expect(affinityOf('')).toBe('BLOB')
  })

  it('REAL / FLOAT / DOUBLE -> REAL', () => {
    expect(affinityOf('REAL')).toBe('REAL')
    expect(affinityOf('FLOAT')).toBe('REAL')
    expect(affinityOf('DOUBLE')).toBe('REAL')
  })

  it('anything else -> NUMERIC (SQLite spec catch-all)', () => {
    expect(affinityOf('NUMERIC')).toBe('NUMERIC')
    expect(affinityOf('DECIMAL')).toBe('NUMERIC')
    expect(affinityOf('DATE')).toBe('NUMERIC')
  })
})

describe('toCanonicalBytes — storage-class-driven canonicalisation', () => {
  it('null and undefined -> empty bytes', () => {
    expect(asArray(toCanonicalBytes(null, 'TEXT'))).toEqual([])
    expect(asArray(toCanonicalBytes(undefined, 'TEXT'))).toEqual([])
    expect(asArray(toCanonicalBytes(null, 'INTEGER'))).toEqual([])
    expect(asArray(toCanonicalBytes(null, 'REAL'))).toEqual([])
    expect(asArray(toCanonicalBytes(null, 'BLOB'))).toEqual([])
  })

  it('INTEGER -> big-endian i64', () => {
    expect(asArray(toCanonicalBytes(0, 'INTEGER'))).toEqual([0, 0, 0, 0, 0, 0, 0, 0])
    expect(asArray(toCanonicalBytes(1, 'INTEGER'))).toEqual([0, 0, 0, 0, 0, 0, 0, 1])
    expect(asArray(toCanonicalBytes(-1, 'INTEGER'))).toEqual([
      0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    ])
    // BigInt input path.
    expect(asArray(toCanonicalBytes(1n, 'INTEGER'))).toEqual([0, 0, 0, 0, 0, 0, 0, 1])
  })

  it('REAL -> big-endian IEEE-754 bits, +0.0 and -0.0 both zero', () => {
    // 1.0 -> 0x3FF0_0000_0000_0000
    expect(asArray(toCanonicalBytes(1.0, 'REAL'))).toEqual([0x3f, 0xf0, 0, 0, 0, 0, 0, 0])
    // -0.0 and +0.0 collapse to eight zero bytes.
    expect(asArray(toCanonicalBytes(-0, 'REAL'))).toEqual([0, 0, 0, 0, 0, 0, 0, 0])
    expect(asArray(toCanonicalBytes(0, 'REAL'))).toEqual([0, 0, 0, 0, 0, 0, 0, 0])
  })

  it('REAL -> NaN normalises to canonical quiet-NaN (0x7FF8_0000_0000_0000)', () => {
    const canonicalNan = [0x7f, 0xf8, 0, 0, 0, 0, 0, 0]
    expect(asArray(toCanonicalBytes(Number.NaN, 'REAL'))).toEqual(canonicalNan)
    // A payload-carrying quiet-NaN from another runtime is normalised too:
    // the payload bits must not survive canonicalisation or two devices
    // would sign different bytes for what SQLite sees as the same Real value.
    const payloadNan = new Float64Array(
      new BigUint64Array([0x7ff8_0000_dead_beefn]).buffer,
    )[0]!
    expect(asArray(toCanonicalBytes(payloadNan, 'REAL'))).toEqual(canonicalNan)
  })

  it('TEXT -> UTF-8 bytes verbatim, no normalisation', () => {
    expect(asArray(toCanonicalBytes('', 'TEXT'))).toEqual([])
    // "he" + U+0301 (combining acute) -> 4 bytes UTF-8
    const decomposed = 'é'
    // "é" as U+00E9 -> 2 bytes UTF-8
    const composed = 'é'
    expect(asArray(toCanonicalBytes(composed, 'TEXT'))).toEqual([0xc3, 0xa9])
    expect(asArray(toCanonicalBytes(decomposed, 'TEXT'))).toEqual([0x65, 0xcc, 0x81])
    // NFC vs NFD must stay distinct (matches Rust's `String::as_bytes`).
    expect(asArray(toCanonicalBytes(composed, 'TEXT'))).not.toEqual(
      asArray(toCanonicalBytes(decomposed, 'TEXT')),
    )
  })

  it('BLOB -> verbatim bytes from Uint8Array or number[]', () => {
    expect(asArray(toCanonicalBytes(new Uint8Array([0, 1, 2, 255]), 'BLOB'))).toEqual([
      0, 1, 2, 255,
    ])
    // Tauri IPC delivers blobs as `number[]` when the row was serialised
    // as JSON — this path is exercised by the scanner's row loader.
    expect(asArray(toCanonicalBytes([0, 1, 2, 255], 'BLOB'))).toEqual([0, 1, 2, 255])
  })

  it('NUMERIC affinity falls back to TEXT encoding', () => {
    expect(asArray(toCanonicalBytes('42', 'NUMERIC'))).toEqual([0x34, 0x32])
  })

  it('BLOB rejects unsupported JS types', () => {
    expect(() => toCanonicalBytes({}, 'BLOB')).toThrow(TypeError)
  })

  it('INTEGER rejects unsupported JS types', () => {
    expect(() => toCanonicalBytes({}, 'INTEGER')).toThrow(TypeError)
  })
})

describe('toCanonicalBase64 / bytesToBase64Standard', () => {
  it('integer 42 -> base64 of 8-byte BE', () => {
    // 0x000000000000002A -> base64 "AAAAAAAAACo="
    expect(toCanonicalBase64(42, 'INTEGER')).toBe('AAAAAAAAACo=')
  })

  it('roundtrips arbitrary bytes through base64-STANDARD (matches Rust engine)', () => {
    // Rust `base64::engine::general_purpose::STANDARD.encode([0xde, 0xad, 0xbe, 0xef])`
    // -> "3q2+7w=="
    expect(bytesToBase64Standard(new Uint8Array([0xde, 0xad, 0xbe, 0xef]))).toBe('3q2+7w==')
  })

  it('empty input -> empty base64 string', () => {
    expect(bytesToBase64Standard(new Uint8Array(0))).toBe('')
    expect(toCanonicalBase64(null, 'TEXT')).toBe('')
  })
})
