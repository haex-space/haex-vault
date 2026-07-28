/**
 * Cross-language column-sig vector test (Phase 1, Task I3).
 *
 * Consumes the same `src-tauri/tests/fixtures/column_sig_vectors.json`
 * that the Rust vector test (Task I2) uses, and asserts that TS's
 * `toCanonicalBytes` + `bytesToBase64Standard` produce byte-identical
 * output to Rust's `to_canonical_bytes` for every SQLite storage class.
 *
 * What this test catches (that unit tests in `columnSigCanonical.test.ts`
 * cannot):
 *   - Drift between the TS canonical encoder and the fixture the Rust
 *     verifier consumes. If either side drifts (Rust preimage layout,
 *     TS storage-class dispatch, base64 alphabet, NaN normalisation,
 *     Unicode handling) this test fails at the exact vector name.
 *   - Layer-0 pre-decrypt sig-presence gate is not exercised here — the
 *     wire-shape verify path is covered by `apply-column-sig-verify.test.ts`.
 *     This file is a pure byte-identity guard.
 *
 * NOT exercised here: Ed25519 signature verification. Signature checks
 * live in Rust (`crdt::column_sig::verify::verify_column_sig` and the
 * `verify_column_sig_batch` Tauri command). vitest has no Tauri IPC
 * harness in this codebase, so we don't invoke the batch verifier from
 * TS — I2 (Rust vector test) is the crypto-layer counterpart. Cross-lang
 * byte-identity is the pre-requisite the sig check depends on, and that
 * is what this file locks down.
 */
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import { bytesToBase64Standard, toCanonicalBytes } from '~/utils/columnSigCanonical'

// ---------------------------------------------------------------------------
// Fixture types (mirror the Rust `Vector` deserialiser).
// ---------------------------------------------------------------------------
interface FixtureValueBlob {
  blob: number[]
}
interface FixtureValueInteger {
  integer: string
}
interface FixtureValueReal {
  real: number
}
interface FixtureValueRealNaN {
  realNaN: true
}
interface FixtureValueText {
  text: string
}
type FixtureValue =
  | null
  | FixtureValueBlob
  | FixtureValueInteger
  | FixtureValueReal
  | FixtureValueRealNaN
  | FixtureValueText

interface Vector {
  name: string
  spaceId: string
  tableName: string
  rowPks: string
  columnName: string
  columnType: string
  hlc: string
  authorDid: string
  value: FixtureValue
  valueBytes: string
  sig: string
  expected:
    | 'verify_ok'
    | 'verify_rejected_sig'
    | 'verify_rejected_wrong_space'
    | 'verify_rejected_wrong_did'
}

interface Fixture {
  domain_tag: string
  vectors: Vector[]
}

// ---------------------------------------------------------------------------
// Reconstruct the JS value that `toCanonicalBytes` expects, from the
// discriminated shape the fixture stores. Kept as a compact dispatch so a
// future storage class is one arm to add on both sides in tandem.
// ---------------------------------------------------------------------------
function reconstructValue(v: FixtureValue): unknown {
  if (v === null) return null
  if ('integer' in v) {
    // TS-side `toCanonicalBytes` for INTEGER accepts bigint or number;
    // the fixture stores the integer as a decimal string so values
    // outside `Number.MAX_SAFE_INTEGER` (deferred IMPORTANT #3 from
    // Runde 7) still round-trip losslessly.
    return BigInt(v.integer)
  }
  if ('realNaN' in v) return Number.NaN
  if ('real' in v) return v.real
  if ('text' in v) return v.text
  if ('blob' in v) return new Uint8Array(v.blob)
  throw new Error(`unhandled fixture value kind: ${JSON.stringify(v)}`)
}

// ---------------------------------------------------------------------------
// Load the fixture once. It lives under src-tauri/, so we walk up from
// this test file to the worktree root and back down.
// ---------------------------------------------------------------------------
const fixturePath = resolve(
  dirname(fileURLToPath(import.meta.url)),
  '..',
  '..',
  '..',
  'src-tauri',
  'tests',
  'fixtures',
  'column_sig_vectors.json',
)
const fixture: Fixture = JSON.parse(readFileSync(fixturePath, 'utf8')) as Fixture

describe('cross-language column-sig fixture — TS canonicalisation matches Rust', () => {
  it('fixture is non-empty and self-consistent', () => {
    // If the file was truncated to 0 vectors, this test would silently
    // pass everything below. Guard explicitly.
    expect(fixture.vectors.length).toBeGreaterThan(0)
    expect(fixture.domain_tag).toBe('haex/space-col-sig/v1')
  })

  for (const v of fixture.vectors) {
    it(`vector "${v.name}" canonicalises to fixture bytes`, () => {
      const reconstructed = reconstructValue(v.value)
      const tsBytes = toCanonicalBytes(reconstructed, v.columnType)
      const tsBase64 = bytesToBase64Standard(tsBytes)
      // Byte-identity is the load-bearing property: any drift here means
      // the sig produced by the TS push path (canonicalises value → base64
      // → Rust builds preimage) will fail Rust verification. That would
      // wedge shared-space pull, so this comparison is a hard gate.
      expect(tsBase64).toBe(v.valueBytes)
    })
  }

  it('multi-space vectors share value+bytes but not sigs', () => {
    // The fixture guarantees a and b have the same canonicalised value
    // (that is the point of the multi-space vector pair). The Rust I2
    // test asserts their sigs differ; here we mirror the byte-identity
    // half so a stale fixture with mismatched valueBytes fails on the
    // TS side too.
    const a = fixture.vectors.find((v) => v.name === 'multi_space_primary_valid')
    const b = fixture.vectors.find((v) => v.name === 'multi_space_secondary_valid')
    expect(a).toBeDefined()
    expect(b).toBeDefined()
    expect(a!.valueBytes).toBe(b!.valueBytes)
    expect(a!.sig).not.toBe(b!.sig)
    expect(a!.authorDid).not.toBe(b!.authorDid)
    expect(a!.spaceId).not.toBe(b!.spaceId)
  })
})
