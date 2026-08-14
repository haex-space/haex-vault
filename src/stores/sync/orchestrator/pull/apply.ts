/**
 * Sync Pull — verification + apply primitives
 */

import { invoke } from '@tauri-apps/api/core'
import { decryptCrdtData } from '@haex-space/vault-sdk'
import { type SpaceCap } from '@haex-space/ucan'
import { eq, and } from 'drizzle-orm'
import type { ColumnChange } from '../../tableScanner'
import { getTableSchemaAsync } from '../../tableScanner'
import { toCanonicalBase64 } from '@/utils/columnSigCanonical'
import { hlcIsNewer } from '@/utils/hlc'
import { orchestratorLog as log } from '../types'
import { splitCompleteGroups, groupByHlcAscending } from '../pull-helpers'
import { haexUcanTokens } from '~/database/schemas'
import { requireDb } from '~/stores/vault'

/**
 * A pulled change that failed verification. `reason` is a stable variant
 * name — either
 *
 *   - a `VerifyColumnSigError` variant echoed back from Rust's
 *     `verify_column_sig_batch` (`MalformedDid`, `InvalidSignature`,
 *     `MalformedSignatureBytes`, `ValueBytesTooLarge`) plus the
 *     batch-layer-only `MalformedValueBytes` (base64 decode failure);
 *   - a `UcanVerifyError` variant surfaced by the Rust chain walker
 *     (`Signature`, `Expired`, `WrongSpace`, `ChainTooDeep`, `ChainBroken`,
 *     `DelegationMissing`, `DelegationNotDelegatable`, `RootNotSelfSigned`,
 *     `RootBindingMismatch`, `RootBindingMalformed`, `MalformedToken`,
 *     `AudienceMismatch`, `EmptyExpectedAudience`, `MissingCapability`,
 *     `InsufficientCapability`, `UnknownCapability`);
 *   - a synthetic reason this TS layer contributes (`Unsigned`,
 *     `MissingLocalUcan`, `MissingResult`).
 */
export interface RejectedChange {
  rowId: string        // synthetic composite: `${tableName}|${rowPks}|${columnName}|${hlcTimestamp}`
  tableName: string
  columnName: string
  rowPks: string
  reason: string
}

// IPC contract — keep in sync with `src-tauri/src/ucan/commands.rs`.
// Rust structs use `#[serde(rename_all = "camelCase")]`, so field names
// are camelCase on the wire despite being snake_case in Rust source.
interface VerifyChainRequest {
  token: string
  expectedSpaceId: string
  expectedAudience: string
  capabilityNeeded: SpaceCap
  rowId: string
  tableName: string
}

type VerifyOutcome =
  | { kind: 'ok'; rootDid: string }
  | { kind: 'rejected'; reason: string }

interface VerifyChainResult {
  rowId: string
  tableName: string
  outcome: VerifyOutcome
}

/**
 * Runtime validator for `VerifyChainResult`. The `invoke<T>` return type is
 * a bare cast — if Rust returns a malformed shape (IPC drop, refactor drift)
 * we want a clear early error, not a cryptic TypeError deep in the consumer.
 * Hand-rolled to avoid pulling in a runtime-validation dep for one shape.
 */
const isVerifyChainResult = (v: unknown): v is VerifyChainResult => {
  if (!v || typeof v !== 'object') return false
  const r = v as {
    rowId?: unknown
    outcome?: { kind?: unknown; rootDid?: unknown; reason?: unknown }
  }
  if (typeof r.rowId !== 'string') return false
  if (!r.outcome || typeof r.outcome !== 'object') return false
  if (r.outcome.kind === 'ok') return typeof r.outcome.rootDid === 'string'
  if (r.outcome.kind === 'rejected') return typeof r.outcome.reason === 'string'
  return false
}

/**
 * Bridge: does the DB row's decomposed `capability` column hold `needed`?
 *
 * The `haex_ucan_tokens.capability` column is a hierarchical string
 * (`space/read`, `space/write`, ...). Task 8b will migrate the column to
 * a serialized `SpaceCapabilitySet`, at which point this check becomes
 * `holdsSpaceCap(deserialize(row.capability), needed)`. Until then, the
 * string-equality bridge is correct — each row is one-cap-per-row.
 *
 * TODO(Task 8b): swap for `holdsSpaceCap` from `@haex-space/ucan` once
 * `capability` is deserialized into a `SpaceCapabilitySet`.
 */
const rowHoldsCap = (rowCapability: string, needed: SpaceCap): boolean =>
  rowCapability === `space/${needed}`

/**
 * Composite correlation key for a `ColumnChange`. Rust echoes this back
 * via `rowId` on each `VerifyChainResult` so we can pair outcomes with
 * their input changes without maintaining an index map.
 */
const rowKey = (c: ColumnChange): string =>
  `${c.tableName}|${c.rowPks}|${c.columnName}|${c.hlcTimestamp}`

const rejectedFrom = (c: ColumnChange, reason: string): RejectedChange => ({
  rowId: rowKey(c),
  tableName: c.tableName,
  columnName: c.columnName,
  rowPks: c.rowPks,
  reason,
})

/**
 * Batch column-sig verify request/response shapes. Mirrors the Rust
 * command in `src-tauri/src/crdt/column_sig/commands.rs`. Field names
 * are camelCase on the wire because every Rust wire struct uses
 * `#[serde(rename_all = "camelCase")]`.
 */
interface ColumnSigChangeWire {
  tableName: string
  rowPks: string
  columnName: string
  hlcTimestamp: string
  valueBytes: string
  sig: { authorDid: string; sig: string; storageClass: 'integer' | 'real' | 'text' | 'blob' | 'null' }
}

interface VerifyColumnSigBatchOutput {
  verified: string[]
  rejected: Array<{ rowKey: string; reason: string }>
}

/**
 * Verifies sig-presence + UCAN chains on pulled changes.
 *
 * Row-scoped, not batch-scoped: a single poisoned row no longer aborts
 * the whole page. Each change is sorted into `verified` (safe to apply)
 * or `rejected` (dropped with a stable reason). Callers apply only
 * `verified`; the log is written per page via `logRejectedChanges` and
 * the aggregated toast fires once at the end of the pull via
 * `surfaceRejectedBatch`.
 *
 * Layers:
 *   0. Layer-0 gate (shared-space only): `sig` must be present. Missing
 *      → reject with `Unsigned`. This is a cheap pre-decrypt gate;
 *      cryptographic verify runs later against the decrypted value.
 *   1. Layer-1 (column signature): NOT here. It moved into
 *      `applyRemoteChangesInTransactionAsync` because the preimage
 *      needs the *decrypted* canonical bytes — shipping those on the
 *      wire alongside `encryptedValue` would leak plaintext to the sync
 *      relay (ADR 0002 §2). See `verifyColumnSigsAgainstDecryptedAsync`.
 *   2. Layer-2 (UCAN chain, shared spaces only): for each sig-bearing
 *      change, look up any locally-cached UCAN in `haex_ucan_tokens`
 *      addressed to the signer (`sig.authorDid`) for this space, then
 *      hand the batch to Rust's `verify_ucan_chain_batch`. Rust walks
 *      the `prf` chain to a self-signed root and verifies the Phase-0
 *      `space_id` binding on that root.
 *
 * Personal-vault sync (`spaceId === undefined`) skips all layers:
 * Phase 1 does not sign personal-vault changes, and there is no UCAN
 * chain to walk when the batch is scoped to the owner's own vault.
 */
export const verifyPulledChangesAsync = async (
  changes: ColumnChange[],
  spaceId: string | undefined,
  currentIdentityDid: string,
  capabilityNeeded: SpaceCap = 'write',
): Promise<{ verified: ColumnChange[]; rejected: RejectedChange[] }> => {
  if (changes.length === 0) return { verified: [], rejected: [] }

  // Non-shared-space pulls (owner-vault sync) skip authenticity + authz:
  // Phase 1 signs only shared-space rows, and there is no UCAN chain
  // outside a space. The batch flows through untouched — the outer
  // HTTP-level DID-auth header already authorised the whole request.
  if (!spaceId) return { verified: changes, rejected: [] }

  const rejected: RejectedChange[] = []

  // === Layer-0: sig-presence gate ===
  // Old peers that pushed before Phase 1 Runde 7 don't carry a `sig`;
  // reject as `Unsigned` so a malicious push can't piggy-back on the
  // missing-verify path. Note: presence-only — the cryptographic verify
  // happens post-decrypt in `verifyColumnSigsAgainstDecryptedAsync`,
  // which is called by `applyRemoteChangesInTransactionAsync`.
  const withSig: ColumnChange[] = []
  for (const change of changes) {
    if (!change.sig) {
      rejected.push(rejectedFrom(change, 'Unsigned'))
      continue
    }
    withSig.push(change)
  }
  if (withSig.length === 0) return { verified: [], rejected }

  // === Layer-2: UCAN chain verify via Rust. ===
  // The signer DID comes straight from `sig.authorDid` — the column-sig
  // already carries a canonical DID (no SPKI-to-DID derivation).
  const db = requireDb()
  const tokenBySigner = new Map<string, string | null>()
  const requests: VerifyChainRequest[] = []
  const layer1AwaitingRust: ColumnChange[] = []

  for (const change of withSig) {
    const signerDid = change.sig!.authorDid
    let token = tokenBySigner.get(signerDid)
    if (token === undefined) {
      const rows = await db
        .select()
        .from(haexUcanTokens)
        .where(
          and(
            eq(haexUcanTokens.spaceId, spaceId),
            eq(haexUcanTokens.audienceDid, signerDid),
          ),
        )
      // Multiple cached UCANs for a signer is legal (e.g. one per capability
      // in the pre-Task-8b decomposed schema, or later, distinct sets with
      // overlap). Pick any token whose capabilities include the one this
      // pull needs; if several match, prefer the one closest to expiry
      // (least-privilege intent: burn down the soonest-expiring valid
      // token first, keep longer-lived ones in reserve).
      const capable = rows.filter((r) => rowHoldsCap(r.capability, capabilityNeeded))
      const chosen = capable.length === 0
        ? null
        : capable.sort((a, b) => a.expiresAt - b.expiresAt)[0]
      token = chosen?.token ?? null
      tokenBySigner.set(signerDid, token)
    }
    if (!token) {
      rejected.push(rejectedFrom(change, 'MissingLocalUcan'))
      continue
    }
    requests.push({
      token,
      expectedSpaceId: spaceId,
      expectedAudience: currentIdentityDid,
      capabilityNeeded,
      rowId: rowKey(change),
      tableName: change.tableName,
    })
    layer1AwaitingRust.push(change)
  }

  if (requests.length === 0) {
    // Everything either failed layer 1 or had no local UCAN. Rust invoke
    // would be a no-op.
    return { verified: [], rejected }
  }

  const results = await invoke<VerifyChainResult[]>('verify_ucan_chain_batch', {
    requests,
  })

  // Guard the IPC boundary: bare `invoke<T>()` is a cast, not validation.
  // A malformed shape here would otherwise blow up further down with a
  // cryptic TypeError; fail fast with a clear message instead.
  if (!Array.isArray(results) || !results.every(isVerifyChainResult)) {
    throw new Error('verify_ucan_chain_batch returned malformed shape')
  }

  const outcomeById = new Map<string, VerifyOutcome>()
  for (const r of results) outcomeById.set(r.rowId, r.outcome)

  const verified: ColumnChange[] = []
  // Iterate the input-order list so `verified` preserves the arrival
  // order the applier expects (per-HLC grouping downstream relies on it).
  for (const change of layer1AwaitingRust) {
    const outcome = outcomeById.get(rowKey(change))
    if (!outcome) {
      rejected.push(rejectedFrom(change, 'MissingResult'))
      continue
    }
    if (outcome.kind === 'ok') {
      verified.push(change)
    } else {
      rejected.push(rejectedFrom(change, outcome.reason))
    }
  }

  return { verified, rejected }
}

/**
 * Structured warn log for a `rejected` list from {@link verifyPulledChangesAsync}
 * (Task 5 contract, unchanged shape).
 *
 * Log-only: the user-visible toast is fired separately by the streaming
 * caller via {@link surfaceRejectedBatch} once per pull batch, so a pull
 * that spans N pages emits N structured log lines (useful for debugging)
 * but at most one toast (the aggregate). No-ops on empty input.
 */
export const logRejectedChanges = (
  rejected: RejectedChange[],
  ctx: { spaceId: string | undefined; backendId: string },
): void => {
  if (rejected.length === 0) return
  log.warn('sync.verification.rows_rejected', {
    space_id: ctx.spaceId ?? null,
    backend_id: ctx.backendId,
    count: rejected.length,
    rows: rejected.map((r) => ({
      row_id: r.rowId,
      table: r.tableName,
      column: r.columnName,
      reason: r.reason,
    })),
  })
}

/**
 * Fires ONE aggregated warning toast for a completed pull batch (Task 6).
 *
 * The count is the sum across all pages of the streaming pull, not a single
 * page — a poisoned pull of 1000 rejects spread over 10 pages surfaces as
 * one toast with count=1000, never ten stacked toasts. The `{count}`
 * interpolation carries the volume; the structured log (per page, via
 * {@link logRejectedChanges}) carries the detail.
 *
 * No-op on `count === 0` so callers can unconditionally fire this after a
 * pull without an outer guard. Empty pulls stay silent.
 *
 * i18n keys `sync.verification.rowsRejected{One,Other}` are merged into
 * the active locale by `useSyncOrchestratorStore` at store init time, so
 * they are guaranteed to exist by the time a pull runs.
 */
export const surfaceRejectedBatch = (
  _spaceId: string | undefined,
  count: number,
): void => {
  if (count <= 0) return
  const { add: addToast } = useToast()
  const { $i18n } = useNuxtApp()
  const key = count === 1
    ? 'sync.verification.rowsRejectedOne'
    : 'sync.verification.rowsRejectedOther'
  addToast({
    title: $i18n.t(key, { count }) as string,
    color: 'warning',
    icon: 'i-lucide-shield-alert',
  })
}

/**
 * Applies ONE page from the streaming pull loop.
 *
 * - `extensionChanges` (haex_extensions) and `migrationChanges`
 *   (haex_extension_migrations) are applied per HLC-group, ext-first, mig-next
 *   (the FK and ordering constraints the previous batch-apply enforced).
 *   They are always small per page, so no hold-back is needed.
 * - If either bucket had any changes on THIS page, materialize new tables via
 *   `apply_synced_extension_migrations` + `ensure_extension_triggers` so the
 *   page's "other" changes can land in the now-existing tables.
 * - `otherChanges` (everything else) is combined with the hold-back carried
 *   over from the prior page, split into "complete-and-apply" vs
 *   "trailing-hold-back" via `splitCompleteGroups`, and the complete portion
 *   is applied per HLC-group (one DB transaction per source transaction).
 *
 * Returns the new hold-back to carry into the next page. Throws on a real
 * apply / verification failure, which leaves the caller's cursor at the prior
 * page (failure isolation).
 */
export async function applyPageAsync(opts: {
  page: ColumnChange[]
  otherHoldBack: ColumnChange[]
  hasMore: boolean
  encryptionKey: Uint8Array
  backendId: string
  spaceId: string
}): Promise<{ holdBack: ColumnChange[]; pageMaxHlc: string }> {
  const { page, otherHoldBack, hasMore, encryptionKey, backendId, spaceId } = opts

  const extensionChanges = page.filter((c) => c.tableName === 'haex_extensions')
  const migrationChanges = page.filter((c) => c.tableName === 'haex_extension_migrations')
  const pageOther = page.filter(
    (c) => c.tableName !== 'haex_extensions' && c.tableName !== 'haex_extension_migrations',
  )

  // Track the max HLC actually applied (vs merely received) so the caller can
  // expose it for the initial-pull case where `lastPushHlcTimestamp` must
  // bracket what was just pulled.
  let pageMaxHlc = ''
  const considerHlc = (hlc: string) => {
    if (hlcIsNewer(hlc, pageMaxHlc)) pageMaxHlc = hlc
  }

  // Extension registrations FIRST (FK target of haex_extension_migrations), then
  // migration definitions, then materialize. Both buckets are small per page —
  // they apply in one HLC-group transaction each.
  if (extensionChanges.length > 0) {
    await applyChangesPerHlcGroupAsync(extensionChanges, encryptionKey, backendId, spaceId)
    for (const c of extensionChanges) considerHlc(c.hlcTimestamp)
  }
  if (migrationChanges.length > 0) {
    await applyChangesPerHlcGroupAsync(migrationChanges, encryptionKey, backendId, spaceId)
    for (const c of migrationChanges) considerHlc(c.hlcTimestamp)
  }
  if (extensionChanges.length > 0 || migrationChanges.length > 0) {
    const migrationResult = await invoke<{
      appliedCount: number
      alreadyAppliedCount: number
      appliedMigrations: string[]
    }>('apply_synced_extension_migrations')
    if (migrationResult.appliedCount > 0) {
      log.info(
        `Applied ${migrationResult.appliedCount} synced extension migrations:`,
        migrationResult.appliedMigrations,
      )
      const triggersCreated = await invoke<number>('ensure_extension_triggers')
      if (triggersCreated > 0) log.info(`Created CRDT triggers for ${triggersCreated} extension tables`)
    }
  }

  // Combine the carried-over hold-back with this page's "other" changes, then
  // split: everything strictly-below the buffer's max HLC applies now; the
  // trailing max-HLC group is held back across the page boundary (or applied
  // now if `hasMore = false`).
  const combined = otherHoldBack.concat(pageOther)
  const { toApply, holdBack } = splitCompleteGroups(combined, hasMore)
  if (toApply.length > 0) {
    await applyChangesPerHlcGroupAsync(toApply, encryptionKey, backendId, spaceId)
    for (const c of toApply) considerHlc(c.hlcTimestamp)
  }
  return { holdBack, pageMaxHlc }
}

/**
 * Applies a flat list of changes per HLC-group (one DB transaction per source
 * transaction). Decryption is done per group, not all at once, so the receive
 * side never holds the full pull delta in memory.
 *
 * A decryption failure inside a group aborts that group with a clear error
 * (no partial apply); a failure in any group aborts the call, leaving prior
 * groups committed — failure isolation at the group level.
 */
async function applyChangesPerHlcGroupAsync(
  changes: ColumnChange[],
  vaultKey: Uint8Array,
  backendId: string,
  spaceId: string,
): Promise<void> {
  if (changes.length === 0) return
  const groups = groupByHlcAscending(changes)
  for (const group of groups) {
    await applyRemoteChangesInTransactionAsync(group.changes, vaultKey, backendId, spaceId)
  }
}


/**
 * Applies remote changes atomically in ONE DB transaction. Used as the inner
 * primitive by both the recurring streaming pull (`pullFromBackendAsync` →
 * `streamPullAndApplyAsync` → `applyPageAsync` → `applyChangesPerHlcGroupAsync`)
 * and the initial-pull path. CALLERS are responsible for splitting by HLC
 * group so each invocation is one source transaction (≤ 100 MB, ADR 0001);
 * passing the entire pull delta here would defeat that.
 *
 * Returns the max HLC timestamp from the changes (for updating
 * `lastPushHlcTimestamp` on the initial-pull path so the pulled data is not
 * re-pushed).
 */
export const applyRemoteChangesInTransactionAsync = async (
  changes: ColumnChange[],
  vaultKey: Uint8Array,
  backendId: string,
  spaceId?: string,
): Promise<string> => {
  const startTime = performance.now()
  log.info(`[PERF] Starting decryption of ${changes.length} changes...`)

  // Cache epoch keys to avoid repeated Tauri calls
  const epochKeyCache = new Map<number, Uint8Array>()

  const resolveDecryptionKey = async (change: ColumnChange): Promise<Uint8Array> => {
    if (change.epoch == null) return vaultKey

    const cached = epochKeyCache.get(change.epoch)
    if (cached) return cached

    const epochKey = await invoke<{ epoch: number; key: number[] }>('mls_get_epoch_key', {
      spaceId: spaceId ?? '',
      epoch: change.epoch,
    })
    const key = new Uint8Array(epochKey.key)
    epochKeyCache.set(change.epoch, key)
    return key
  }

  // Calculate max HLC and decrypt all changes
  let maxHlc = ''
  const decryptedChanges = []
  let decryptCount = 0
  const failedDecryptions: Array<{ tableName: string; columnName: string; error: unknown }> = []

  for (const change of changes) {
    decryptCount++
    // Log every 100 changes for better visibility with smaller batches
    if (decryptCount % 100 === 0 || decryptCount === changes.length) {
      const elapsed = (performance.now() - startTime) / 1000
      const rate = decryptCount / elapsed
      log.info(`[PERF] Decrypted ${decryptCount}/${changes.length} (${elapsed.toFixed(1)}s, ${rate.toFixed(0)} changes/s)`)
    }
    // Track max HLC
    if (hlcIsNewer(change.hlcTimestamp, maxHlc)) {
      maxHlc = change.hlcTimestamp
    }

    // Decrypt the value — use epoch key if available, else vault key
    let decryptedValue
    if (change.encryptedValue && change.nonce) {
      try {
        const decryptionKey = await resolveDecryptionKey(change)
        const decryptedData = await decryptCrdtData<{ value: unknown }>(
          change.encryptedValue,
          change.nonce,
          decryptionKey,
        )
        decryptedValue = decryptedData.value
      } catch (err) {
        // CRITICAL: Collect decryption failures - we will abort the entire transaction
        // Skipping individual changes would cause data inconsistency
        failedDecryptions.push({
          tableName: change.tableName,
          columnName: change.columnName,
          error: err,
        })
        log.error(`Failed to decrypt change for ${change.tableName}.${change.columnName}:`, err)
        continue
      }
    } else {
      // No encrypted value means the value is intentionally null (e.g., cleared field)
      decryptedValue = null
    }

    const changeObj = {
      tableName: change.tableName,
      rowPks: change.rowPks,
      columnName: change.columnName,
      hlcTimestamp: change.hlcTimestamp,
      decryptedValue,
      // Retained on the intermediate struct (not sent to Rust) so the
      // post-decrypt column-sig verify below has the wire sig to pair
      // with the freshly-decrypted plaintext.
      sig: change.sig,
    }

    decryptedChanges.push(changeObj)
  }

  // CRITICAL: If ANY decryption failed, abort the entire transaction
  // This ensures data consistency - we don't want to partially apply changes
  if (failedDecryptions.length > 0) {
    const errorDetails = failedDecryptions
      .slice(0, 5) // Show first 5 failures
      .map((f) => `${f.tableName}.${f.columnName}`)
      .join(', ')
    const moreCount = failedDecryptions.length > 5 ? ` (+${failedDecryptions.length - 5} more)` : ''

    throw new Error(
      `Decryption failed for ${failedDecryptions.length} change(s): ${errorDetails}${moreCount}. ` +
      `Transaction aborted to maintain data consistency. ` +
      `This may indicate an incorrect vault key or corrupted data on the server.`,
    )
  }

  const decryptionTime = (performance.now() - startTime) / 1000
  log.info(`[PERF] Decryption complete in ${decryptionTime.toFixed(1)}s. Max HLC: ${maxHlc}`)

  // === Column-sig verify (Layer 1) — POST-DECRYPT, shared-space only. ===
  // The preimage over `(space_id, table, row_pks, column, hlc,
  // author_did, canonical_value_bytes)` uses the *decrypted* value's
  // canonical bytes, which never leave this device — shipping them
  // alongside `encryptedValue` on the wire would defeat the ADR §2
  // confidentiality guarantee against the sync relay. Rust is the
  // single verifier; TS canonicalises locally, batches, and filters
  // rejects by row_key. Personal-vault sync (no `spaceId`) has no
  // signatures to check.
  const applicableForSig = spaceId
    ? decryptedChanges.filter((c) => c.sig)
    : []
  let verifiedDecrypted = decryptedChanges
  if (applicableForSig.length > 0) {
    // Cache per-table column-type lookups. `sig`-bearing changes
    // usually cluster on a handful of tables per HLC group, so this
    // is bounded (typically ≤ 6 shared-space tables).
    const columnTypeByTable = new Map<string, Map<string, string>>()
    const resolveColumnType = async (
      tableName: string,
      columnName: string,
    ): Promise<string | undefined> => {
      let table = columnTypeByTable.get(tableName)
      if (!table) {
        const schema = await getTableSchemaAsync(tableName)
        table = new Map(schema.map((c) => [c.name, c.type]))
        columnTypeByTable.set(tableName, table)
      }
      return table.get(columnName)
    }

    const sigChanges: ColumnSigChangeWire[] = []
    const sigDropped: Array<{ rowKey: string; reason: string }> = []
    const seenSigRowKeys = new Set<string>()
    for (const c of applicableForSig) {
      const rowKey = `${c.tableName}|${c.rowPks}|${c.columnName}|${c.hlcTimestamp}`
      if (seenSigRowKeys.has(rowKey)) {
        // The verifier correlates results by this key, so two entries would
        // be ambiguous. Marking the key rejected drops every duplicate
        // without wedging the pull cursor on attacker-controlled input.
        sigDropped.push({ rowKey, reason: 'DuplicateRowKey' })
        continue
      }
      seenSigRowKeys.add(rowKey)
      const columnType = await resolveColumnType(c.tableName, c.columnName)
      if (!columnType) {
        // Local schema drift — the column no longer exists in the
        // receiving schema. Drop the change with a synthetic reason
        // rather than send a mismatched preimage to Rust.
        sigDropped.push({ rowKey, reason: 'MissingResult' })
        continue
      }
      // A decrypted value that cannot be canonicalised (wrong JS type for the
      // column's affinity, a non-byte entry in a blob array, a non-numeric
      // string in an INTEGER column) is one bad change, not a bad batch.
      // Letting `toCanonicalBase64` throw here would reject the whole pull
      // transaction and wedge the cursor behind it.
      let valueBytes: string
      try {
        valueBytes = toCanonicalBase64(
          c.decryptedValue,
          columnType,
          c.sig!.storageClass,
        )
      } catch (err) {
        log.warn(`canonicalisation failed for ${rowKey}:`, err)
        sigDropped.push({ rowKey, reason: 'MalformedValueBytes' })
        continue
      }
      sigChanges.push({
        tableName: c.tableName,
        rowPks: c.rowPks,
        columnName: c.columnName,
        hlcTimestamp: c.hlcTimestamp,
        valueBytes,
        sig: c.sig!,
      })
    }

    const sigResult = await invoke<VerifyColumnSigBatchOutput>(
      'verify_column_sig_batch',
      { input: { changes: sigChanges, expectedSpaceId: spaceId! } },
    )
    if (
      !sigResult
      || !Array.isArray(sigResult.verified)
      || !Array.isArray(sigResult.rejected)
    ) {
      throw new Error('verify_column_sig_batch returned malformed shape')
    }

    const verifiedSet = new Set(sigResult.verified)
    const rejectReasonByKey = new Map<string, string>()
    for (const r of sigResult.rejected) rejectReasonByKey.set(r.rowKey, r.reason)
    for (const d of sigDropped) rejectReasonByKey.set(d.rowKey, d.reason)

    verifiedDecrypted = decryptedChanges.filter((c) => {
      // Personal-vault-scoped rows on a shared backend (unsigned) are
      // out-of-band and never reach here because the outer verify
      // layer requires `sig` for shared-space pulls — but be defensive.
      if (!c.sig) return true
      const key = `${c.tableName}|${c.rowPks}|${c.columnName}|${c.hlcTimestamp}`
      const explicitRejection = rejectReasonByKey.get(key)
      if (explicitRejection) {
        log.warn(`column-sig verify rejected ${key}: ${explicitRejection}`)
        return false
      }
      if (verifiedSet.has(key)) return true
      log.warn(`column-sig verify rejected ${key}: MissingResult`)
      return false
    })
    const dropped = decryptedChanges.length - verifiedDecrypted.length
    if (dropped > 0) {
      log.warn(`column-sig verify dropped ${dropped} change(s) in this transaction`)
    }
  }

  // Keep the verified signature attached: Rust persists it beside the
  // applied value so this receiver can later relay the original author's
  // change without re-signing it.
  const applyPayload = verifiedDecrypted

  log.info(`[PERF] Invoking Rust: apply_remote_changes_in_transaction (${applyPayload.length} changes)`)

  // Call Tauri command to apply changes in a transaction
  const rustStartTime = performance.now()
  try {
    await invoke('apply_remote_changes_in_transaction', {
      changes: applyPayload,
      backendId,
      maxHlc,
      // Pull scope. Rust needs it to anchor column-sig verification for rows
      // that do not exist locally yet; without it the only available anchor
      // would be the batch's own (unauthenticated) `space_id` column change.
      spaceId: spaceId ?? null,
    })
    const rustTime = (performance.now() - rustStartTime) / 1000
    log.info(`[PERF] Rust command completed in ${rustTime.toFixed(1)}s`)
  } catch (invokeError) {
    // Log detailed error from Rust - extract message for better visibility
    const errorMessage = invokeError instanceof Error
      ? invokeError.message
      : typeof invokeError === 'object' && invokeError !== null
        ? JSON.stringify(invokeError, null, 2)
        : String(invokeError)
    log.error('Rust command apply_remote_changes_in_transaction failed:', errorMessage)
    log.error('Full error object:', invokeError)
    throw invokeError
  }

  return maxHlc
}
