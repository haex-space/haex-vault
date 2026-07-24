/**
 * Sync Pull — verification + apply primitives
 */

import { invoke } from '@tauri-apps/api/core'
import { decryptCrdtData, verifyRecordSignatureAsync, publicKeyToDidKeyAsync } from '@haex-space/vault-sdk'
import { eq, and } from 'drizzle-orm'
import type { ColumnChange } from '../../tableScanner'
import { hlcIsNewer } from '@/utils/hlc'
import { orchestratorLog as log } from '../types'
import { splitCompleteGroups, groupByHlcAscending } from '../pull-helpers'
import { haexUcanTokens } from '~/database/schemas'
import { requireDb } from '~/stores/vault'

/**
 * Capability levels understood by the Rust chain-walker
 * (`src-tauri/src/ucan/verify.rs::CapabilityLevel`, `#[serde(rename_all = "snake_case")]`).
 */
export type CapabilityLevel = 'read' | 'write' | 'invite' | 'admin'

/**
 * A pulled change that failed verification. `reason` is a stable variant
 * name — either a `UcanVerifyError` variant surfaced by the Rust chain
 * walker (`Signature`, `Expired`, `WrongSpace`, `ChainTooDeep`,
 * `ChainBroken`, `CapabilityEscalation`, `RootNotSelfSigned`,
 * `RootBindingMismatch`, `RootBindingMalformed`, `MalformedToken`,
 * `AudienceMismatch`, `EmptyExpectedAudience`, `MissingCapability`,
 * `InsufficientCapability`, `UnknownCapability`) or one of the synthetic
 * reasons this TS layer contributes (`Unsigned`, `InvalidRecordSignature`,
 * `MissingLocalUcan`, `MissingResult`).
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
  capabilityNeeded: CapabilityLevel
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
 * Ordinal rank for a stored capability string, used to pick the strongest
 * cached UCAN when a signer has multiple tokens for the same space. The
 * column stores full `space/*` names; unknown values rank 0 so future
 * capabilities do not silently outrank current ones.
 */
const capabilityRank = (cap: string): number => {
  switch (cap) {
    case 'space/read': return 1
    case 'space/write': return 2
    case 'space/invite': return 3
    case 'space/admin': return 4
    default: return 0
  }
}

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
 * Verifies signatures + UCAN chains on pulled changes.
 *
 * Row-scoped, not batch-scoped: a single poisoned row no longer aborts
 * the whole page. Each change is sorted into `verified` (safe to apply)
 * or `rejected` (dropped with a stable reason). Callers apply only
 * `verified` and surface `rejected` counts to the user (Task 6 will
 * add a toast; today we log).
 *
 * Layers:
 *   0. Layer-0 gate: `signature` and `signedBy` must be present. Missing
 *      either → reject with `Unsigned`.
 *   1. Layer-1 (record signature): the Ed25519 signature over
 *      `(tableName, rowPks, columnName, encryptedValue, hlcTimestamp)`
 *      must verify against `signedBy`. Broken → reject with
 *      `InvalidRecordSignature`.
 *   2. Layer-2 (UCAN chain, shared spaces only): for each surviving
 *      change, look up any locally-cached UCAN in `haex_ucan_tokens`
 *      addressed to the signer for this space, then hand the batch to
 *      Rust's `verify_ucan_chain_batch`. Rust is the single source of
 *      chain-walking truth — it walks the `prf` chain to a self-signed
 *      root and verifies the Phase-0 `space_id` binding on that root.
 *
 * The old admin fallback (an inline "self-signed root UCAN grants full
 * authority" TS bypass) has been removed. Even a space owner's root
 * UCAN must now bind to a self-certifying `space_id` (ADR 0002 §Phase-0)
 * via the Rust chain walker.
 */
export const verifyPulledChangesAsync = async (
  changes: ColumnChange[],
  spaceId: string | undefined,
  currentIdentityDid: string,
  capabilityNeeded: CapabilityLevel = 'write',
): Promise<{ verified: ColumnChange[]; rejected: RejectedChange[] }> => {
  if (changes.length === 0) return { verified: [], rejected: [] }

  const rejected: RejectedChange[] = []
  const passedLayer1: ColumnChange[] = []

  for (const change of changes) {
    if (!change.signature || !change.signedBy) {
      rejected.push(rejectedFrom(change, 'Unsigned'))
      continue
    }
    const isValid = await verifyRecordSignatureAsync(
      {
        tableName: change.tableName,
        rowPks: change.rowPks,
        columnName: change.columnName,
        encryptedValue: change.encryptedValue ?? null,
        hlcTimestamp: change.hlcTimestamp,
      },
      change.signature,
      change.signedBy,
    )
    if (!isValid) {
      rejected.push(rejectedFrom(change, 'InvalidRecordSignature'))
      continue
    }
    passedLayer1.push(change)
  }

  // Non-shared-space pulls (owner-vault sync) stop at layer 1 — no UCAN
  // chain applies, so anything with a valid record signature is verified.
  if (!spaceId) return { verified: passedLayer1, rejected }

  // Resolve signer DIDs and cache one UCAN per signer for this space.
  // A change whose signer has no cached UCAN is rejected outright: we
  // cannot ask Rust to verify a token we do not possess.
  const db = requireDb()
  const tokenBySigner = new Map<string, string | null>()
  const requests: VerifyChainRequest[] = []
  const layer1AwaitingRust: ColumnChange[] = []

  for (const change of passedLayer1) {
    const signerDid = await publicKeyToDidKeyAsync(change.signedBy!)
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
      // level). Pick the highest-capability token so a write-scoped change
      // isn't rejected because a read-only token happened to be picked first
      // (SQLite row order is otherwise arbitrary without an ORDER BY).
      const best = rows.reduce<{ token: string; capability: string } | null>(
        (acc, r) =>
          !acc || capabilityRank(r.capability) > capabilityRank(acc.capability)
            ? r
            : acc,
        null,
      )
      token = best?.token ?? null
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
 * Reports a `rejected` list from {@link verifyPulledChangesAsync}:
 *
 *   1. Structured warn log — machine-readable channel (Task 5, unchanged).
 *   2. Aggregated warning toast — user-visible mirror of the log (Task 6).
 *
 * One toast per batch, not per row: a poisoned page with 1000 rejects must
 * not spam the UI with 1000 stacked toasts. The `{count}` interpolation
 * carries the volume; the structured log carries the detail. The two
 * channels stay in sync because they are triggered from the same call
 * site.
 *
 * i18n keys `sync.verification.rowsRejected{One,Other}` are merged into
 * the active locale by `useSyncOrchestratorStore` at store init time, so
 * they are guaranteed to exist by the time a pull runs.
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

  const { add: addToast } = useToast()
  const { $i18n } = useNuxtApp()
  const key = rejected.length === 1
    ? 'sync.verification.rowsRejectedOne'
    : 'sync.verification.rowsRejectedOther'
  addToast({
    title: $i18n.t(key, { count: rejected.length }) as string,
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
  log.info(`[PERF] Invoking Rust: apply_remote_changes_in_transaction (${decryptedChanges.length} changes)`)

  // Call Tauri command to apply changes in a transaction
  const rustStartTime = performance.now()
  try {
    await invoke('apply_remote_changes_in_transaction', {
      changes: decryptedChanges,
      backendId,
      maxHlc,
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
