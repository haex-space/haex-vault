/**
 * TS port of `src-tauri/src/crdt/column_sig/value_bytes.rs::to_canonical_bytes`.
 *
 * Both sides MUST produce byte-identical output for the same value; the
 * Ed25519 preimage in `preimage.rs` concatenates these bytes and any drift
 * breaks the sig chain across languages. Runde 8 §I locks it down with a
 * cross-language fixture; until then callers depend on the mirroring below.
 *
 * Layout: `storage_class_tag (1 byte) || native byte form`.
 *   - NULL     → `[NULL]`
 *   - INTEGER  → `[INTEGER]` + i64 big-endian, 8 bytes
 *   - REAL     → `[REAL]` + f64 big-endian IEEE-754 bits, NaN → canonical
 *                quiet-NaN (`0x7FF8_0000_0000_0000`), -0.0 → +0.0
 *   - TEXT     → `[TEXT]` + UTF-8 bytes verbatim (no Unicode normalisation)
 *   - BLOB     → `[BLOB]` + bytes verbatim
 *
 * The tag makes each storage class a distinct preimage. Without it `NULL`,
 * `TEXT('')` and `BLOB([])` all encode to the same empty byte string, so a
 * signature captured over one verifies against the others — see the Rust
 * doc comment for the full replay argument.
 *
 * Tauri IPC flattens SQL Integer and SQL Real to the same JS `number`, so
 * storage class cannot be recovered from the arrived value. We drive
 * canonicalisation from the *column's declared type affinity* (SQLite rules)
 * instead — which matches Rust's `Value` discriminator because CRDT columns
 * are written via `execute_with_crdt` and SQLite honours affinity → storage
 * class on ingest.
 *
 * That inference is not total: SQLite permits any storage class in any
 * column (e.g. a non-numeric string stored in an INTEGER-affinity column
 * stays TEXT). Such a row diverges from the signer's class and now fails
 * verification loudly instead of being accepted against a substituted
 * preimage. Restricting signed columns to non-lossy affinity mappings, or
 * carrying the class explicitly, is Phase-3 work.
 */

const CANONICAL_QUIET_NAN = 0x7ff8_0000_0000_0000n

/**
 * Storage-class tags. MUST match
 * `src-tauri/src/crdt/column_sig/value_bytes.rs::tag` — the values are
 * SQLite's own `SQLITE_*` type codes.
 */
export const STORAGE_CLASS_TAG = {
  INTEGER: 1,
  REAL: 2,
  TEXT: 3,
  BLOB: 4,
  NULL: 5,
} as const

function tagged(tag: number, body: ArrayLike<number>): Uint8Array {
  const out = new Uint8Array(1 + body.length)
  out[0] = tag
  out.set(body, 1)
  return out
}

type Affinity = 'INTEGER' | 'REAL' | 'TEXT' | 'BLOB' | 'NUMERIC'
export type SqliteStorageClass = 'integer' | 'real' | 'text' | 'blob' | 'null'

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
  return tagged(STORAGE_CLASS_TAG.INTEGER, new Uint8Array(buf))
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
  return tagged(STORAGE_CLASS_TAG.REAL, new Uint8Array(buf))
}

/**
 * `BigInt(x)` rejects non-integral and non-numeric input with `RangeError`
 * (`Infinity`, `NaN`) or `SyntaxError` (a non-numeric string) rather than
 * the `TypeError` every other branch of `toCanonicalBytes` throws. Callers
 * catch on a single error type to drop one change, so funnel all three into
 * `TypeError` — otherwise an odd value escapes as an unhandled rejection and
 * aborts the whole pull transaction instead of costing one change.
 */
function toI64(value: bigint | number | string, source: string): bigint {
  try {
    if (typeof value === 'bigint') return value
    if (typeof value === 'number') {
      if (!Number.isFinite(value)) {
        throw new TypeError(
          `toCanonicalBytes: ${source} value ${String(value)} is not a finite number`,
        )
      }
      return BigInt(Math.trunc(value))
    }
    return BigInt(value.trim())
  } catch (err) {
    if (err instanceof TypeError) throw err
    throw new TypeError(
      `toCanonicalBytes: ${source} value ${JSON.stringify(value)} is not an integer (${
        err instanceof Error ? err.message : String(err)
      })`,
      { cause: err },
    )
  }
}

/**
 * Encode `value` (raw JS value read from `sql_select`) into the canonical
 * byte form for column-sig preimages. `columnType` is the declared column
 * type from `getTableSchemaAsync` — its affinity drives the discriminator.
 */
export function toCanonicalBytes(
  value: unknown,
  columnType: string,
  storageClass?: SqliteStorageClass,
): Uint8Array {
  if (value === null || value === undefined) {
    if (storageClass && storageClass !== 'null') {
      throw new TypeError(
        `toCanonicalBytes: ${storageClass.toUpperCase()} storage class carried a null value`,
      )
    }
    return new Uint8Array([STORAGE_CLASS_TAG.NULL])
  }
  if (storageClass === 'null') {
    throw new TypeError('toCanonicalBytes: NULL storage class carried a non-null value')
  }

  const affinity = storageClass
    ? ({
        integer: 'INTEGER',
        real: 'REAL',
        text: 'TEXT',
        blob: 'BLOB',
        null: 'TEXT',
      } as const)[storageClass]
    : affinityOf(columnType)

  if (affinity === 'INTEGER') {
    if (typeof value === 'boolean') return encodeI64BE(value ? 1n : 0n)
    if (typeof value === 'bigint' || typeof value === 'number' || typeof value === 'string') {
      return encodeI64BE(toI64(value, 'INTEGER'))
    }
    throw new TypeError(`toCanonicalBytes: unsupported INTEGER value type ${typeof value}`)
  }

  if (affinity === 'REAL') {
    if (typeof value === 'number') return encodeF64BE(value)
    if (typeof value === 'bigint') return encodeF64BE(Number(value))
    if (typeof value === 'string') {
      const parsed = Number.parseFloat(value)
      if (Number.isNaN(parsed) && value.trim().toLowerCase() !== 'nan') {
        throw new TypeError(
          `toCanonicalBytes: REAL value ${JSON.stringify(value)} is not a number`,
        )
      }
      return encodeF64BE(parsed)
    }
    throw new TypeError(`toCanonicalBytes: unsupported REAL value type ${typeof value}`)
  }

  if (affinity === 'BLOB') {
    if (value instanceof Uint8Array) return tagged(STORAGE_CLASS_TAG.BLOB, value)
    if (Array.isArray(value)) {
      // `Uint8Array.from(array)` silently coerces out-of-range or non-integer
      // entries (-1 → 255, 300 → 44, 1.7 → 1). Rust's `Value::Blob(Vec<u8>)`
      // carries verbatim bytes, so a silent coercion here would diverge the
      // preimage across languages. Validate per entry and throw explicitly.
      return tagged(
        STORAGE_CLASS_TAG.BLOB,
        Uint8Array.from(value as unknown[], (v) => {
          if (typeof v !== 'number' || !Number.isInteger(v) || v < 0 || v > 255) {
            throw new TypeError(
              `toCanonicalBytes: BLOB entry ${JSON.stringify(v)} is not a byte (0..=255 integer)`,
            )
          }
          return v
        }),
      )
    }
    if (typeof value === 'string') {
      if (storageClass === 'blob') {
        try {
          const decoded = atob(value)
          return tagged(
            STORAGE_CLASS_TAG.BLOB,
            Uint8Array.from(decoded, (char) => char.charCodeAt(0)),
          )
        }
        catch (err) {
          throw new TypeError(
            `toCanonicalBytes: BLOB value is not base64 (${
              err instanceof Error ? err.message : String(err)
            })`,
            { cause: err },
          )
        }
      }
      return tagged(STORAGE_CLASS_TAG.BLOB, new TextEncoder().encode(value))
    }
    throw new TypeError(`toCanonicalBytes: unsupported BLOB value type ${typeof value}`)
  }

  // TEXT + NUMERIC (SQLite's NUMERIC affinity falls back to TEXT when the
  // value isn't obviously a number, and CRDT rows never store literal
  // number blobs under NUMERIC columns in practice).
  const asText = typeof value === 'string' ? value : String(value)
  return tagged(STORAGE_CLASS_TAG.TEXT, new TextEncoder().encode(asText))
}

/**
 * base64-STANDARD encoding of the canonical bytes. Matches the Rust
 * `base64::engine::general_purpose::STANDARD` used by
 * `crdt::column_sig::commands::verify_column_sig_batch_inner`.
 */
export function toCanonicalBase64(
  value: unknown,
  columnType: string,
  storageClass?: SqliteStorageClass,
): string {
  return bytesToBase64Standard(toCanonicalBytes(value, columnType, storageClass))
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
