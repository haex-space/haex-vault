import { describe, it, expect } from 'vitest'
import {
  affinityOf,
  toCanonicalBytes,
  toCanonicalBase64,
  bytesToBase64Standard,
  STORAGE_CLASS_TAG,
} from '~/utils/columnSigCanonical'

/**
 * These tests lock down cross-language byte identity with
 * `src-tauri/src/crdt/column_sig/value_bytes.rs`. Runde 8 §I adds a shared
 * fixture that exercises both sides against the SAME expected outputs; the
 * vectors here mirror the Rust unit tests (`value_bytes_tests.rs`) so a
 * refactor on either side breaks fast, not silently at sig-verify time.
 *
 * Every canonical encoding starts with a one-byte SQLite storage-class tag,
 * so expected arrays below are written as `[TAG, ...body]`.
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

describe('STORAGE_CLASS_TAG', () => {
  it('matches SQLite type codes and the Rust `value_bytes::tag` module', () => {
    expect(STORAGE_CLASS_TAG).toEqual({
      INTEGER: 1,
      REAL: 2,
      TEXT: 3,
      BLOB: 4,
      NULL: 5,
    })
  })
})

describe('toCanonicalBytes — storage-class-driven canonicalisation', () => {
  it('uses the transported SQLite storage class instead of declared affinity', () => {
    expect(asArray(toCanonicalBytes(1, 'TEXT', 'real'))).toEqual([
      STORAGE_CLASS_TAG.REAL, 0x3f, 0xf0, 0, 0, 0, 0, 0, 0,
    ])
    expect(asArray(toCanonicalBytes('AAEC/w==', 'TEXT', 'blob'))).toEqual([
      STORAGE_CLASS_TAG.BLOB, 0, 1, 2, 255,
    ])
    expect(asArray(toCanonicalBytes('AAEC/w==', 'BLOB', 'text'))).toEqual([
      STORAGE_CLASS_TAG.TEXT, 0x41, 0x41, 0x45, 0x43, 0x2f, 0x77, 0x3d, 0x3d,
    ])
  })

  it('rejects a non-null value labelled with the NULL storage class', () => {
    expect(() => toCanonicalBytes('not-null', 'TEXT', 'null')).toThrow(TypeError)
    expect(() => toCanonicalBytes(null, 'TEXT', 'real')).toThrow(TypeError)
  })

  it('null and undefined -> bare NULL tag, regardless of column affinity', () => {
    const nullBytes = [STORAGE_CLASS_TAG.NULL]
    expect(asArray(toCanonicalBytes(null, 'TEXT'))).toEqual(nullBytes)
    expect(asArray(toCanonicalBytes(undefined, 'TEXT'))).toEqual(nullBytes)
    expect(asArray(toCanonicalBytes(null, 'INTEGER'))).toEqual(nullBytes)
    expect(asArray(toCanonicalBytes(null, 'REAL'))).toEqual(nullBytes)
    expect(asArray(toCanonicalBytes(null, 'BLOB'))).toEqual(nullBytes)
  })

  it('INTEGER -> tag + big-endian i64', () => {
    const t = STORAGE_CLASS_TAG.INTEGER
    expect(asArray(toCanonicalBytes(0, 'INTEGER'))).toEqual([t, 0, 0, 0, 0, 0, 0, 0, 0])
    expect(asArray(toCanonicalBytes(1, 'INTEGER'))).toEqual([t, 0, 0, 0, 0, 0, 0, 0, 1])
    expect(asArray(toCanonicalBytes(-1, 'INTEGER'))).toEqual([
      t, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    ])
    // BigInt input path.
    expect(asArray(toCanonicalBytes(1n, 'INTEGER'))).toEqual([t, 0, 0, 0, 0, 0, 0, 0, 1])
  })

  it('REAL -> tag + big-endian IEEE-754 bits, +0.0 and -0.0 both zero', () => {
    const t = STORAGE_CLASS_TAG.REAL
    // 1.0 -> 0x3FF0_0000_0000_0000
    expect(asArray(toCanonicalBytes(1.0, 'REAL'))).toEqual([t, 0x3f, 0xf0, 0, 0, 0, 0, 0, 0])
    // -0.0 and +0.0 collapse to eight zero bytes.
    expect(asArray(toCanonicalBytes(-0, 'REAL'))).toEqual([t, 0, 0, 0, 0, 0, 0, 0, 0])
    expect(asArray(toCanonicalBytes(0, 'REAL'))).toEqual([t, 0, 0, 0, 0, 0, 0, 0, 0])
  })

  it('REAL -> NaN normalises to canonical quiet-NaN (0x7FF8_0000_0000_0000)', () => {
    const canonicalNan = [STORAGE_CLASS_TAG.REAL, 0x7f, 0xf8, 0, 0, 0, 0, 0, 0]
    expect(asArray(toCanonicalBytes(Number.NaN, 'REAL'))).toEqual(canonicalNan)
    // A payload-carrying quiet-NaN from another runtime is normalised too:
    // the payload bits must not survive canonicalisation or two devices
    // would sign different bytes for what SQLite sees as the same Real value.
    const payloadNan = new Float64Array(
      new BigUint64Array([0x7ff8_0000_dead_beefn]).buffer,
    )[0]!
    expect(asArray(toCanonicalBytes(payloadNan, 'REAL'))).toEqual(canonicalNan)
  })

  it('TEXT -> tag + UTF-8 bytes verbatim, no normalisation', () => {
    const t = STORAGE_CLASS_TAG.TEXT
    expect(asArray(toCanonicalBytes('', 'TEXT'))).toEqual([t])
    // NFC "é" (U+00E9) is 2 UTF-8 bytes; NFD "e" + U+0301 (combining acute)
    // is 3. Both are written as explicit escapes, never as literal glyphs:
    // as literals they are visually identical, so any editor, formatter or
    // tooling step that applies Unicode normalisation would silently collapse
    // them into each other and gut the inequality assertion below.
    const composed = '\u00E9'
    const decomposed = 'e\u0301'
    expect(asArray(toCanonicalBytes(composed, 'TEXT'))).toEqual([t, 0xc3, 0xa9])
    expect(asArray(toCanonicalBytes(decomposed, 'TEXT'))).toEqual([t, 0x65, 0xcc, 0x81])
    // NFC vs NFD must stay distinct (matches Rust's `String::as_bytes`).
    expect(asArray(toCanonicalBytes(composed, 'TEXT'))).not.toEqual(
      asArray(toCanonicalBytes(decomposed, 'TEXT')),
    )
  })

  it('BLOB -> tag + verbatim bytes from Uint8Array or number[]', () => {
    const t = STORAGE_CLASS_TAG.BLOB
    expect(asArray(toCanonicalBytes(new Uint8Array([0, 1, 2, 255]), 'BLOB'))).toEqual([
      t, 0, 1, 2, 255,
    ])
    // Tauri IPC delivers blobs as `number[]` when the row was serialised
    // as JSON — this path is exercised by the scanner's row loader.
    expect(asArray(toCanonicalBytes([0, 1, 2, 255], 'BLOB'))).toEqual([t, 0, 1, 2, 255])
  })

  it('NUMERIC affinity falls back to TEXT encoding', () => {
    expect(asArray(toCanonicalBytes('42', 'NUMERIC'))).toEqual([
      STORAGE_CLASS_TAG.TEXT, 0x34, 0x32,
    ])
  })

  it('storage classes with identical bodies do not collide', () => {
    // Untagged, all three encoded to the same empty byte string, so one
    // signature verified against all three values.
    const empties = [
      asArray(toCanonicalBytes(null, 'TEXT')),
      asArray(toCanonicalBytes('', 'TEXT')),
      asArray(toCanonicalBytes(new Uint8Array(0), 'BLOB')),
    ]
    expect(new Set(empties.map((e) => e.join(','))).size).toBe(3)

    // Integer 1 and the blob of its own 8 big-endian bytes share a body.
    const intOne = asArray(toCanonicalBytes(1, 'INTEGER'))
    const blobOne = asArray(toCanonicalBytes([0, 0, 0, 0, 0, 0, 0, 1], 'BLOB'))
    expect(intOne).not.toEqual(blobOne)
    expect(intOne.slice(1)).toEqual(blobOne.slice(1))
  })

  it('BLOB rejects unsupported JS types', () => {
    expect(() => toCanonicalBytes({}, 'BLOB')).toThrow(TypeError)
  })

  it('BLOB rejects out-of-range or non-integer entries in number[]', () => {
    // Silent coercion here would diverge preimages across languages:
    // -1 would become 255, 300 would become 44, 1.7 would truncate to 1.
    expect(() => toCanonicalBytes([-1], 'BLOB')).toThrow(TypeError)
    expect(() => toCanonicalBytes([300], 'BLOB')).toThrow(TypeError)
    expect(() => toCanonicalBytes([1.7], 'BLOB')).toThrow(TypeError)
    expect(() => toCanonicalBytes([Number.NaN], 'BLOB')).toThrow(TypeError)
  })

  it('INTEGER rejects unsupported JS types', () => {
    expect(() => toCanonicalBytes({}, 'INTEGER')).toThrow(TypeError)
  })

  it('every rejection path throws TypeError, never RangeError or SyntaxError', () => {
    // `BigInt(Infinity)` throws RangeError and `BigInt('abc')` throws
    // SyntaxError natively. Callers catch on one error type to drop a single
    // change; an escaping RangeError/SyntaxError would abort the whole pull
    // transaction instead.
    expect(() => toCanonicalBytes(Number.POSITIVE_INFINITY, 'INTEGER')).toThrow(TypeError)
    expect(() => toCanonicalBytes(Number.NEGATIVE_INFINITY, 'INTEGER')).toThrow(TypeError)
    expect(() => toCanonicalBytes(Number.NaN, 'INTEGER')).toThrow(TypeError)
    expect(() => toCanonicalBytes('not-a-number', 'INTEGER')).toThrow(TypeError)
    expect(() => toCanonicalBytes('1.5', 'INTEGER')).toThrow(TypeError)
    expect(() => toCanonicalBytes('not-a-number', 'REAL')).toThrow(TypeError)
  })

  it('INTEGER accepts numeric strings and booleans', () => {
    const t = STORAGE_CLASS_TAG.INTEGER
    expect(asArray(toCanonicalBytes('42', 'INTEGER'))).toEqual([t, 0, 0, 0, 0, 0, 0, 0, 42])
    expect(asArray(toCanonicalBytes(true, 'INTEGER'))).toEqual([t, 0, 0, 0, 0, 0, 0, 0, 1])
    expect(asArray(toCanonicalBytes(false, 'INTEGER'))).toEqual([t, 0, 0, 0, 0, 0, 0, 0, 0])
  })
})

describe('toCanonicalBase64 / bytesToBase64Standard', () => {
  it('integer 42 -> base64 of tag + 8-byte BE', () => {
    // 0x01 || 0x000000000000002A -> base64 "AQAAAAAAAAAq"
    expect(toCanonicalBase64(42, 'INTEGER')).toBe('AQAAAAAAAAAq')
  })

  it('roundtrips arbitrary bytes through base64-STANDARD (matches Rust engine)', () => {
    // Rust `base64::engine::general_purpose::STANDARD.encode([0xde, 0xad, 0xbe, 0xef])`
    // -> "3q2+7w=="
    expect(bytesToBase64Standard(new Uint8Array([0xde, 0xad, 0xbe, 0xef]))).toBe('3q2+7w==')
  })

  it('empty input -> empty base64 string; NULL -> single tag byte', () => {
    expect(bytesToBase64Standard(new Uint8Array(0))).toBe('')
    // 0x05 -> "BQ=="
    expect(toCanonicalBase64(null, 'TEXT')).toBe('BQ==')
  })
})
