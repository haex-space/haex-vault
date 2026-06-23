/**
 * Sync Pull — verification + apply primitives
 */

import { invoke } from '@tauri-apps/api/core'
import { decryptCrdtData, verifyRecordSignatureAsync, publicKeyToDidKeyAsync } from '@haex-space/vault-sdk'
import {
  validateUcan,
  spaceResource,
  createWebCryptoVerifier,
  type Capability,
} from '@haex-space/ucan'
import { eq, and } from 'drizzle-orm'
import type { ColumnChange } from '../../tableScanner'
import { hlcIsNewer } from '@/utils/hlc'
import { orchestratorLog as log } from '../types'
import { splitCompleteGroups, groupByHlcAscending } from '../pull-helpers'
import { haexUcanTokens } from '~/database/schemas'
import { requireDb } from '~/stores/vault'

/**
 * Error raised when verification of a pulled change-batch fails.
 *
 * The batch is rejected as a whole on first invalid change — we don't trust
 * any change from a batch where a compromised or misconfigured server has
 * tampered with, forged, or skipped signatures for any entry.
 */
export class BatchVerificationError extends Error {
  readonly reason: 'unsigned' | 'invalid-signature' | 'unauthorized'
  readonly offendingChange: { tableName: string; columnName: string; rowPks: string }

  constructor(
    reason: 'unsigned' | 'invalid-signature' | 'unauthorized',
    offendingChange: ColumnChange,
    detail: string,
  ) {
    super(`Batch rejected (${reason}): ${detail}`)
    this.name = 'BatchVerificationError'
    this.reason = reason
    this.offendingChange = {
      tableName: offendingChange.tableName,
      columnName: offendingChange.columnName,
      rowPks: offendingChange.rowPks,
    }
  }
}

/**
 * Verifies signatures and UCAN authorization on pulled changes before applying them.
 *
 * Two-layer verification:
 *   1. Cryptographic: every change has an Ed25519 signature and `signedBy`, and the signature verifies
 *   2. Authorization (shared spaces only): signer holds a valid `space/write` UCAN or is the root admin
 *
 * Fails the entire batch on first invalid change — throws `BatchVerificationError`.
 * All data must be signed; there is no pass-through for unsigned records.
 */
export const verifyPulledChangesAsync = async (
  changes: ColumnChange[],
  spaceId?: string,
): Promise<void> => {
  if (changes.length === 0) return

  const db = requireDb()
  const verify = createWebCryptoVerifier()

  // Cache: signedBy public key → { did, authorized } to avoid re-checking per column change
  const signerCache = new Map<string, { did: string; authorized: boolean }>()

  for (const change of changes) {
    if (!change.signature || !change.signedBy) {
      throw new BatchVerificationError(
        'unsigned',
        change,
        `change ${change.tableName}/${change.columnName} is missing signature or signedBy`,
      )
    }

    // Layer 1: Cryptographic signature verification
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
      throw new BatchVerificationError(
        'invalid-signature',
        change,
        `invalid Ed25519 signature on ${change.tableName}/${change.columnName}`,
      )
    }

    // Layer 2: UCAN authorization check — only for shared spaces
    if (!spaceId) continue

    let cached = signerCache.get(change.signedBy)
    if (!cached) {
      const signerDid = await publicKeyToDidKeyAsync(change.signedBy)

      // Look up a UCAN where this signer is the audience (was granted access)
      // and the capability is at least space/write for this space
      const ucanRows = await db
        .select()
        .from(haexUcanTokens)
        .where(
          and(
            eq(haexUcanTokens.spaceId, spaceId),
            eq(haexUcanTokens.audienceDid, signerDid),
          ),
        )

      let authorized = false
      for (const row of ucanRows) {
        try {
          const result = await validateUcan(
            row.token,
            spaceResource(spaceId),
            'space/write' as Capability,
            verify,
          )
          if (result.valid) {
            authorized = true
            break
          }
        } catch {
          // Token invalid or expired — try next
        }
      }

      // Admin fallback: root UCAN where issuer === audience (self-issued by space owner)
      if (!authorized) {
        const rootUcans = await db
          .select()
          .from(haexUcanTokens)
          .where(
            and(
              eq(haexUcanTokens.spaceId, spaceId),
              eq(haexUcanTokens.issuerDid, signerDid),
              eq(haexUcanTokens.audienceDid, signerDid),
            ),
          )
        authorized = rootUcans.length > 0
      }

      cached = { did: signerDid, authorized }
      signerCache.set(change.signedBy, cached)
    }

    if (!cached.authorized) {
      throw new BatchVerificationError(
        'unauthorized',
        change,
        `signer ${cached.did.slice(0, 24)}... has no valid space/write UCAN for space ${spaceId}`,
      )
    }
  }
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
