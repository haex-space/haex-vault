/**
 * TS port of `src-tauri/src/crdt/column_sig/value_bytes.rs::to_canonical_bytes`.
 *
 * Both sides MUST produce byte-identical output for the same value; the
 * Ed25519 preimage in `preimage.rs` concatenates these bytes and any drift
 * breaks the sig chain across languages. Runde 8 §I locks it down with a
 * cross-language fixture; until then callers depend on the mirroring below.
 *
 * SQLite storage class → bytes:
 *   - NULL     → empty
 *   - INTEGER  → i64 big-endian, 8 bytes
 *   - REAL     → f64 big-endian IEEE-754 bits, NaN → canonical quiet-NaN
 *                (`0x7FF8_0000_0000_0000`), -0.0 normalised to +0.0
 *   - TEXT     → UTF-8 bytes verbatim (no Unicode normalisation)
 *   - BLOB     → bytes verbatim
 *
 * Tauri IPC flattens SQL Integer and SQL Real to the same JS `number`, so
 * storage class cannot be recovered from the arrived value. We drive
 * canonicalisation from the *column's declared type affinity* (SQLite rules)
 * instead — which matches Rust's `Value` discriminator because CRDT columns
 * are written via `execute_with_crdt` and SQLite honours affinity → storage
 * class on ingest.
 */

const CANONICAL_QUIET_NAN = 0x7ff8_0000_0000_0000n

type Affinity = 'INTEGER' | 'REAL' | 'TEXT' | 'BLOB' | 'NUMERIC'

/**
 * SQLite type-affinity rules (§3.1 of the SQLite datatype spec), matching
 * `sqlite3AffinityType()` in the SQLite C source. The order matters:
 * INT wins over REAL wins over TEXT wins over BLOB.
 */
export function affinityOf(columnType: string): Affinity {
  const t = columnType.toUpperCase()
  if (t.includes('INT')) return 'INTEGER'
  if (t.includes('CHAR') || t.includes('CLOB') || t.includes('TEXT')) return 'TEXT'
  if (t.includes('BLOB') || t === '') return 'BLOB'
  if (t.includes('REAL') || t.includes('FLOA') || t.includes('DOUB')) return 'REAL'
  return 'NUMERIC'
}

function encodeI64BE(n: bigint): Uint8Array {
  const buf = new ArrayBuffer(8)
  new DataView(buf).setBigInt64(0, n, false)
  return new Uint8Array(buf)
}

function encodeF64BE(n: number): Uint8Array {
  const buf = new ArrayBuffer(8)
  const view = new DataView(buf)
  if (Number.isNaN(n)) {
    view.setBigUint64(0, CANONICAL_QUIET_NAN, false)
  } else if (n === 0) {
    // Kills -0.0 as well since -0.0 === 0 under JS ===.
    view.setBigUint64(0, 0n, false)
  } else {
    view.setFloat64(0, n, false)
  }
  return new Uint8Array(buf)
}

/**
 * Encode `value` (raw JS value read from `sql_select`) into the canonical
 * byte form for column-sig preimages. `columnType` is the declared column
 * type from `getTableSchemaAsync` — its affinity drives the discriminator.
 */
export function toCanonicalBytes(value: unknown, columnType: string): Uint8Array {
  if (value === null || value === undefined) return new Uint8Array(0)

  const affinity = affinityOf(columnType)

  if (affinity === 'INTEGER') {
    if (typeof value === 'bigint') return encodeI64BE(value)
    if (typeof value === 'number') return encodeI64BE(BigInt(Math.trunc(value)))
    if (typeof value === 'boolean') return encodeI64BE(value ? 1n : 0n)
    if (typeof value === 'string') return encodeI64BE(BigInt(value))
    throw new TypeError(`toCanonicalBytes: unsupported INTEGER value type ${typeof value}`)
  }

  if (affinity === 'REAL') {
    if (typeof value === 'number') return encodeF64BE(value)
    if (typeof value === 'bigint') return encodeF64BE(Number(value))
    if (typeof value === 'string') return encodeF64BE(Number.parseFloat(value))
    throw new TypeError(`toCanonicalBytes: unsupported REAL value type ${typeof value}`)
  }

  if (affinity === 'BLOB') {
    if (value instanceof Uint8Array) return new Uint8Array(value)
    if (Array.isArray(value)) {
      // `Uint8Array.from(array)` silently coerces out-of-range or non-integer
      // entries (-1 → 255, 300 → 44, 1.7 → 1). Rust's `Value::Blob(Vec<u8>)`
      // carries verbatim bytes, so a silent coercion here would diverge the
      // preimage across languages. Validate per entry and throw explicitly.
      return Uint8Array.from(value as unknown[], (v) => {
        if (typeof v !== 'number' || !Number.isInteger(v) || v < 0 || v > 255) {
          throw new TypeError(
            `toCanonicalBytes: BLOB entry ${JSON.stringify(v)} is not a byte (0..=255 integer)`,
          )
        }
        return v
      })
    }
    if (typeof value === 'string') return new TextEncoder().encode(value)
    throw new TypeError(`toCanonicalBytes: unsupported BLOB value type ${typeof value}`)
  }

  // TEXT + NUMERIC (SQLite's NUMERIC affinity falls back to TEXT when the
  // value isn't obviously a number, and CRDT rows never store literal
  // number blobs under NUMERIC columns in practice).
  const asText = typeof value === 'string' ? value : String(value)
  return new TextEncoder().encode(asText)
}

/**
 * base64-STANDARD encoding of the canonical bytes. Matches the Rust
 * `base64::engine::general_purpose::STANDARD` used by
 * `crdt::column_sig::commands::verify_column_sig_batch_inner`.
 */
export function toCanonicalBase64(value: unknown, columnType: string): string {
  return bytesToBase64Standard(toCanonicalBytes(value, columnType))
}

/**
 * base64-STANDARD encoder for raw bytes. Kept separate so callers who
 * already hold a `Uint8Array` (e.g. re-encoding a decrypted blob) don't
 * have to round-trip through `toCanonicalBytes`.
 */
export function bytesToBase64Standard(bytes: Uint8Array): string {
  // btoa reads its argument as Latin-1: each char code becomes one byte,
  // so 0..255 map 1:1 into the base64 output.
  let bin = ''
  for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]!)
  return btoa(bin)
}
