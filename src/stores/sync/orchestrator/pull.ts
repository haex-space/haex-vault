/**
 * Sync Pull Operations
 * Handles pulling remote changes from the sync server
 */

import { invoke } from '@tauri-apps/api/core'
import { emit } from '@tauri-apps/api/event'
import { fetch } from '@tauri-apps/plugin-http'
import { decryptCrdtData, verifyRecordSignatureAsync, publicKeyToDidKeyAsync } from '@haex-space/vault-sdk'
import {
  DidAuthAction,
  validateUcan,
  spaceResource,
  createWebCryptoVerifier,
  type Capability,
} from '@haex-space/ucan'
import { eq, and } from 'drizzle-orm'
import type { ColumnChange } from '../tableScanner'
import { hlcIsNewer } from '@/utils/hlc'
import { createDidAuthHeader, createFederatedDidAuthHeader } from '@/utils/auth/didAuth'
import { orchestratorLog as log, type BackendSyncState, syncMutex } from './types'
import { splitCompleteGroups, groupByHlcAscending } from './pull-helpers'
import { useExtensionBroadcastStore } from '~/stores/extensions/broadcast'
import { SYNC_TABLES_INTERNAL_EVENT } from '../syncEvents'
import { haexUcanTokens } from '~/database/schemas'
import { requireDb } from '~/stores/vault'
import type { PendingColumn } from '@bindings/PendingColumn'

/**
 * Pulls changes from a specific backend using column-level HLC comparison.
 *
 * Streams pages from the server and applies each one immediately, splitting
 * the page's "other-table" changes into per-source-transaction groups (one
 * HLC = one transaction) and committing each group in its own DB transaction.
 * The trailing (max-HLC) group of a page is held back across the page boundary
 * until the next page — or `hasMore = false` — confirms it is complete, so a
 * source transaction is never applied in pieces. This keeps the receive-side
 * memory bounded (one page + one held-back transaction, not the whole history)
 * and ensures one bad transaction can't halt the entire sync: a failing group
 * leaves the cursor at the last successfully-applied page, and the next pull
 * cycle resumes from there.
 */
export const pullFromBackendAsync = async (
  backendId: string,
  currentVaultId: string | undefined,
  syncStates: BackendSyncState,
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>,
  syncEngineStore: ReturnType<typeof useSyncEngineStore>,
): Promise<void> => {
  log.info(`========== PULL START (backend: ${backendId}) ==========`)

  if (!currentVaultId) {
    log.error('PULL FAILED: No vault opened')
    throw new Error('No vault opened')
  }

  const state = syncStates[backendId]
  if (!state) {
    log.error('PULL FAILED: Backend not initialized')
    throw new Error('Backend not initialized')
  }

  // Acquire mutex lock to prevent concurrent sync operations
  const releaseLock = await syncMutex.acquire(backendId)
  state.isSyncing = true
  state.error = null

  try {
    const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
    if (!backend?.spaceId) {
      log.error('PULL FAILED: Backend spaceId not configured')
      throw new Error('Backend spaceId not configured')
    }

    // Get encryption key: vault sync key from local DB
    const encryptionKey = await syncEngineStore.getSyncKeyFromDbAsync(backendId)
    if (!encryptionKey) {
      log.error('PULL FAILED: Vault sync key not available')
      throw new Error('Vault sync key not available')
    }

    const lastPullServerTimestamp = backend.lastPullServerTimestamp
    log.debug('Pull config:', {
      backendId,
      spaceId: backend.spaceId,
      lastPullServerTimestamp: lastPullServerTimestamp || '(none - full sync)',
    })

    const federation = backend.type === 'relay' && backend.homeServerDid && backend.originServerDid
      ? { serverDid: backend.homeServerDid, originServerDid: backend.originServerDid }
      : undefined

    const streamResult = await streamPullAndApplyAsync({
      homeServerUrl: backend.homeServerUrl,
      spaceId: backend.spaceId,
      initialCursor: lastPullServerTimestamp || null,
      encryptionKey,
      backendId,
      backendIdentityId: backend.identityId,
      federation,
      // Persist the cursor per FULLY-APPLIED page so a mid-stream failure
      // leaves the next cycle resuming from the last good page (re-applying
      // already-committed groups is a CRDT LWW no-op).
      onPageCommitted: async (pageServerTimestamp) => {
        if (pageServerTimestamp) {
          await syncBackendsStore.updateBackendAsync(backendId, {
            lastPullServerTimestamp: pageServerTimestamp,
          })
        }
      },
    })

    const { totalApplied, pageCount, tablesAffected } = streamResult
    if (totalApplied === 0) {
      log.info('PULL COMPLETE: No new changes from server')
      return
    }

    log.info(`Streamed ${totalApplied} changes across ${pageCount} pages (tables: ${tablesAffected.size})`)

    // Pull any pending columns (older app version skipped columns that now
    // exist after migrations). This runs AFTER the main streaming apply so the
    // base data is in place; it has its own pagination + apply path.
    const pendingColumnsPulled = await pullPendingColumnsAsync(
      backend.homeServerUrl,
      backend.spaceId,
      encryptionKey,
      backendId,
      syncEngineStore,
    )
    if (pendingColumnsPulled > 0) {
      log.info(`Pulled ${pendingColumnsPulled} pending column changes`)
    }

    // `lastPullServerTimestamp` was advanced per page inside the loop above; no
    // end-of-cycle update is needed.

    log.debug('Reloading backend config after pull...')
    await syncBackendsStore.loadBackendsAsync()

    // Emit sync events for store reloading + extensions.
    const tables = [...tablesAffected]
    if (tables.length > 0) {
      log.info('Emitting sync:tables-updated events for tables:', tables)
      // Internal event drives main-window store reloads.
      await emit(SYNC_TABLES_INTERNAL_EVENT, { tables })
      log.info('Internal sync:tables-updated event emitted for store reloading')

      // Filtered per-extension events (each extension only sees its tables).
      const broadcastStore = useExtensionBroadcastStore()
      await broadcastStore.broadcastSyncTablesUpdated(tables)
      log.info('Filtered sync:tables-updated events emitted to extensions')
    }

    if (tablesAffected.has('haex_device_mls_enrollments')) {
      const deviceStore = useDeviceStore()
      if (deviceStore.deviceId) {
        import('@/composables/useDeviceEnrollment').then(({ useDeviceEnrollment }) => {
          const { syncEnrollmentsAsync } = useDeviceEnrollment()
          syncEnrollmentsAsync(deviceStore.deviceId!).catch((e) => log.debug('Device enrollment sync failed:', e))
        })
      }
    }

    log.info(`========== PULL SUCCESS: ${totalApplied} changes applied across ${pageCount} pages ==========`)
  } catch (error) {
    // Extract detailed error message for better debugging
    let errorMessage = 'Unknown error'
    if (error instanceof Error) {
      errorMessage = error.message
      // Check if it's a Tauri invoke error with more details
      if ('cause' in error && error.cause) {
        errorMessage += ` (cause: ${JSON.stringify(error.cause)})`
      }
    } else if (typeof error === 'object' && error !== null) {
      errorMessage = JSON.stringify(error)
    }
    log.error(`========== PULL FAILED ==========`, { message: errorMessage, stack: error instanceof Error ? error.stack : undefined })
    state.error = errorMessage
    throw error
  } finally {
    state.isSyncing = false
    releaseLock()
  }
}

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
const verifyPulledChangesAsync = async (
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
 * Fetches ONE page of changes from the server. Stable pagination uses a
 * 3-cursor composite `(afterUpdatedAt, afterTableName, afterRowPks)` — same as
 * the server's `orderBy(asc(max(updated_at)), asc(tableName), asc(rowPks))`.
 *
 * The caller drives the page loop (see `pullFromBackendAsync`); this function
 * is intentionally stateless so a failure inside the loop doesn't lose any
 * page's cursor.
 */
interface FetchPullPageOptions {
  homeServerUrl: string
  spaceId: string
  cursor: string | null
  cursorTableName: string | null
  cursorRowPks: string | null
  backendIdentityId?: string | null
  federation?: { serverDid: string; originServerDid: string }
}

interface PullPage {
  changes: ColumnChange[]
  hasMore: boolean
  serverTimestamp: string | null
  lastTableName: string | null
  lastRowPks: string | null
}

const fetchPullPageAsync = async (options: FetchPullPageOptions): Promise<PullPage> => {
  const { homeServerUrl, spaceId, cursor, cursorTableName, cursorRowPks, backendIdentityId, federation } = options

  // Resolve identity for DID-Auth (getIdentityByIdAsync falls back to in-memory when vault not open).
  let identity: { privateKey: string; did: string } | null = null
  if (backendIdentityId) {
    const identityStore = useIdentityStore()
    const id = await identityStore.getIdentityByIdAsync(backendIdentityId)
    if (id?.privateKey) identity = { privateKey: id.privateKey, did: id.did }
  }
  if (!identity) throw new Error('No identity configured for this backend')

  const params = new URLSearchParams({ spaceId, limit: '1000' })
  if (cursor) params.set('afterUpdatedAt', cursor)
  if (cursorTableName) params.set('afterTableName', cursorTableName)
  if (cursorRowPks) params.set('afterRowPks', cursorRowPks)

  const queryString = params.toString()
  const authHeader = federation
    ? await createFederatedDidAuthHeader({
        did: identity.did,
        privateKeyBase64: identity.privateKey,
        action: DidAuthAction.SyncPull,
        federation: { spaceId, serverDid: federation.originServerDid, relayDid: federation.serverDid },
        queryString,
      })
    : await createDidAuthHeader(identity.privateKey, identity.did, DidAuthAction.SyncPull)

  const response = await fetch(`${homeServerUrl}/sync/pull?${queryString}`, {
    method: 'GET',
    headers: { Authorization: authHeader },
  })

  if (!response.ok) {
    const error = await response.json().catch(() => ({}))
    log.error('Server returned error:', { status: response.status, error })
    throw new Error(`Failed to pull changes: ${error.error || response.statusText}`)
  }

  const data = await response.json()
  return {
    changes: data.changes || [],
    hasMore: data.hasMore === true,
    serverTimestamp: data.serverTimestamp || null,
    lastTableName: data.lastTableName || null,
    lastRowPks: data.lastRowPks || null,
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
async function applyPageAsync(opts: {
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
 * Stream pages from the server, verifying+decrypting+applying each one
 * immediately. Used by both the recurring pull (`pullFromBackendAsync`) and
 * the initial-pull path in `orchestrator/index.ts`, with `onPageCommitted`
 * supplied only by callers that have a persisted cursor to advance.
 *
 * Returns the running max HLC across all pages (consumed by the initial-pull
 * caller to set `lastPushHlcTimestamp` so the pulled data is not re-pushed)
 * plus per-cycle stats (affected tables, totals).
 */
export interface StreamPullOptions {
  homeServerUrl: string
  spaceId: string
  initialCursor: string | null
  encryptionKey: Uint8Array
  backendId: string
  backendIdentityId?: string | null
  federation?: { serverDid: string; originServerDid: string }
  /** Called after each fully-applied page so a caller with a persisted backend
   *  can advance its cursor (failure-isolation: the cursor stays at the last
   *  successful page if a later one throws). */
  onPageCommitted?: (pageServerTimestamp: string | null) => Promise<void>
}

export interface StreamPullResult {
  totalApplied: number
  pageCount: number
  tablesAffected: Set<string>
  maxHlc: string
  lastServerTimestamp: string | null
}

export const streamPullAndApplyAsync = async (opts: StreamPullOptions): Promise<StreamPullResult> => {
  const { homeServerUrl, spaceId, initialCursor, encryptionKey, backendId, backendIdentityId, federation, onPageCommitted } = opts

  let cursor: string | null = initialCursor
  let cursorTableName: string | null = null
  let cursorRowPks: string | null = null
  let hasMore = true
  let otherHoldBack: ColumnChange[] = []
  const tablesAffected = new Set<string>()
  let pageCount = 0
  let totalApplied = 0
  let maxHlc = ''
  let lastServerTimestamp: string | null = initialCursor

  log.info('Streaming pull-and-apply from server...')
  while (hasMore) {
    pageCount++

    const page = await fetchPullPageAsync({
      homeServerUrl,
      spaceId,
      cursor,
      cursorTableName,
      cursorRowPks,
      backendIdentityId,
      federation,
    })

    hasMore = page.hasMore
    log.info(
      `[STREAM] Page ${pageCount}: ${page.changes.length} changes, ` +
      `hasMore=${hasMore}, serverTimestamp=${page.serverTimestamp}`,
    )

    // Stall guard: a correct server returning `hasMore=true` always advances at
    // least one composite-cursor component. If none moved (empty page with
    // has_more, or a buggy/replaying server), continuing would spin this loop
    // forever. Stop; the next cycle retries from the last successfully-applied
    // page (or from the initial cursor for the initial-pull path).
    if (
      hasMore &&
      page.changes.length === 0 &&
      page.serverTimestamp === cursor &&
      page.lastTableName === cursorTableName &&
      page.lastRowPks === cursorRowPks
    ) {
      log.warn(
        `[STREAM] Server claims hasMore=true but cursor did not advance ` +
        `(serverTimestamp=${page.serverTimestamp}); stopping to avoid an infinite pull loop`,
      )
      break
    }

    // Verify signatures+UCAN on this page. A bad change aborts the whole
    // stream — the outer caller leaves the cursor wherever the last successful
    // page committed it (or unchanged for the initial-pull path).
    await verifyPulledChangesAsync(page.changes, spaceId)

    for (const c of page.changes) tablesAffected.add(c.tableName)

    const result = await applyPageAsync({
      page: page.changes,
      otherHoldBack,
      hasMore,
      encryptionKey,
      backendId,
      spaceId,
    })
    otherHoldBack = result.holdBack
    if (hlcIsNewer(result.pageMaxHlc, maxHlc)) maxHlc = result.pageMaxHlc

    totalApplied += page.changes.length

    if (onPageCommitted) await onPageCommitted(page.serverTimestamp)

    lastServerTimestamp = page.serverTimestamp
    cursor = page.serverTimestamp
    cursorTableName = page.lastTableName
    cursorRowPks = page.lastRowPks
  }

  return { totalApplied, pageCount, tablesAffected, maxHlc, lastServerTimestamp }
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

/**
 * Pulls data for pending columns that were skipped during sync
 *
 * When a device has an older schema version and receives changes with unknown columns,
 * those columns are tracked in haex_crdt_pending_columns_no_sync. After the app updates
 * and migrations add those columns, this function fetches ALL data for them from the server.
 *
 * @param homeServerUrl - Sync server URL
 * @param spaceId - Space ID to pull data for
 * @param vaultKey - Vault encryption key for decryption
 * @param backendId - Backend ID for applying changes
 * @param syncEngineStore - Sync engine store for auth token
 * @returns Number of columns successfully pulled
 */
export const pullPendingColumnsAsync = async (
  homeServerUrl: string,
  spaceId: string,
  vaultKey: Uint8Array,
  backendId: string,
  _syncEngineStore: ReturnType<typeof useSyncEngineStore>,
): Promise<number> => {
  // Step 1: Get list of pending columns from local database
  const pendingColumns = await invoke<PendingColumn[]>('get_pending_columns')

  if (pendingColumns.length === 0) {
    log.debug('No pending columns to pull')
    return 0
  }

  log.info(`Found ${pendingColumns.length} pending columns to pull:`, pendingColumns)

  const syncBackendsStore = useSyncBackendsStore()
  const backendRecord = syncBackendsStore.backends.find(b => b.id === backendId)
  const identityStore = useIdentityStore()
  const resolved = backendRecord?.identityId ? await identityStore.getIdentityByIdAsync(backendRecord.identityId) : null
  if (!resolved?.privateKey) throw new Error('No identity configured for this backend')
  const identity = { privateKey: resolved.privateKey, did: resolved.did }

  const federation = backendRecord?.type === 'relay' && backendRecord.homeServerDid && backendRecord.originServerDid
    ? { serverDid: backendRecord.homeServerDid, originServerDid: backendRecord.originServerDid }
    : undefined

  // Step 2: Pull data for each column from server (with pagination)
  let totalPulled = 0

  for (const pendingCol of pendingColumns) {
    log.info(`Pulling data for column: ${pendingCol.tableName}.${pendingCol.columnName}`)

    const allChanges: ColumnChange[] = []
    let hasMore = true
    let lastTableName: string | undefined
    let lastRowPks: string | undefined

    // Pagination loop for this column
    while (hasMore) {
      const requestBody = JSON.stringify({
        spaceId,
        columns: [{ tableName: pendingCol.tableName, columnName: pendingCol.columnName }],
        limit: 1000,
        afterTableName: lastTableName,
        afterRowPks: lastRowPks,
      })
      const authHeader = federation
        ? await createFederatedDidAuthHeader({
            did: identity.did,
            privateKeyBase64: identity.privateKey,
            action: DidAuthAction.SyncPullColumns,
            federation: { spaceId, serverDid: federation.originServerDid, relayDid: federation.serverDid },
            body: requestBody,
          })
        : await createDidAuthHeader(identity.privateKey, identity.did, DidAuthAction.SyncPullColumns, requestBody)

      const response = await fetch(`${homeServerUrl}/sync/pull-columns`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Authorization: authHeader },
        body: requestBody,
      })

      if (!response.ok) {
        const error = await response.json().catch(() => ({}))
        log.error(`Failed to pull column ${pendingCol.tableName}.${pendingCol.columnName}:`, error)
        throw new Error(`Failed to pull column data: ${error.error || response.statusText}`)
      }

      const data = await response.json()
      const changes: ColumnChange[] = data.changes || []

      allChanges.push(...changes)
      hasMore = data.hasMore === true
      lastTableName = data.lastTableName
      lastRowPks = data.lastRowPks

      log.debug(`Fetched ${changes.length} changes for ${pendingCol.tableName}.${pendingCol.columnName} (total: ${allChanges.length}, hasMore: ${hasMore})`)
    }

    // Step 3: Verify signatures + UCAN authorization, then apply.
    // A BatchVerificationError here aborts pullPendingColumnsAsync before clear_pending_column
    // runs, so this pending column stays flagged for retry on the next pull.
    if (allChanges.length > 0) {
      await verifyPulledChangesAsync(allChanges, spaceId)
      log.info(`Applying ${allChanges.length} changes for ${pendingCol.tableName}.${pendingCol.columnName}`)
      await applyRemoteChangesInTransactionAsync(allChanges, vaultKey, backendId, spaceId)
      totalPulled += allChanges.length
    }

    // Step 4: Clear this pending column from tracking table
    await invoke('clear_pending_column', {
      tableName: pendingCol.tableName,
      columnName: pendingCol.columnName,
    })
    log.info(`Cleared pending column: ${pendingCol.tableName}.${pendingCol.columnName}`)
  }

  log.info(`Finished pulling pending columns. Total changes applied: ${totalPulled}`)
  return totalPulled
}
