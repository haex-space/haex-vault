/**
 * Sync Pull — page fetch + paginated streaming
 */

import { fetch } from '@tauri-apps/plugin-http'
import type { ColumnChange } from '../../tableScanner'
import { hlcIsNewer } from '@/utils/hlc'
import { createDidAuthHeader, createFederatedDidAuthHeader } from '@/utils/auth/didAuth'
import { orchestratorLog as log } from '../types'
import { applyPageAsync, verifyPulledChangesAsync, logRejectedChanges, surfaceRejectedBatch } from './apply'

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
  const url = `${homeServerUrl}/sync/pull?${queryString}`
  const authHeader = federation
    ? await createFederatedDidAuthHeader({
        did: identity.did,
        privateKeyBase64: identity.privateKey,
        federation: { spaceId, serverDid: federation.originServerDid, relayDid: federation.serverDid },
        method: 'GET',
        path: '/sync/pull',
        rawQuery: queryString,
        body: '',
      })
    : await createDidAuthHeader(identity.privateKey, identity.did, {
        method: 'GET',
        url,
      })

  const response = await fetch(url, {
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

  // Resolve the current backend identity DID up front — this is the
  // `expected_audience` for every UCAN in this pull. Batching amortises
  // the identity-store lookup across all pages.
  let currentIdentityDid = ''
  if (backendIdentityId) {
    const identityStore = useIdentityStore()
    const id = await identityStore.getIdentityByIdAsync(backendIdentityId)
    if (id?.did) currentIdentityDid = id.did
  }
  if (!currentIdentityDid) throw new Error('No identity configured for this backend')

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
  // Accumulator across pages: sum of `rejected.length` from every page. The
  // toast fires ONCE after the loop with this total, so a pull that spans N
  // pages emits at most one toast instead of N stacked ones. On early
  // exit (throw/break) the try/finally below still surfaces whatever was
  // accumulated so far — the user is not silently kept in the dark.
  let totalRejected = 0

  log.info('Streaming pull-and-apply from server...')
  try {
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

    // Verify signatures+UCAN on this page. Row-scoped: a poisoned row is
    // dropped from `verified` and logged in `rejected`; the rest of the
    // page still applies and the cursor still advances. Log rejections
    // per page (machine-readable channel, Task 5); the aggregated toast
    // fires once at the end of the pull via `surfaceRejectedBatch`.
    const { verified, rejected } = await verifyPulledChangesAsync(
      page.changes,
      spaceId,
      currentIdentityDid,
      'write',
    )
    totalRejected += rejected.length
    logRejectedChanges(rejected, { spaceId, backendId })

    for (const c of verified) tablesAffected.add(c.tableName)

    const result = await applyPageAsync({
      page: verified,
      otherHoldBack,
      hasMore,
      encryptionKey,
      backendId,
      spaceId,
    })
    otherHoldBack = result.holdBack
    if (hlcIsNewer(result.pageMaxHlc, maxHlc)) maxHlc = result.pageMaxHlc

    totalApplied += verified.length

    if (onPageCommitted) await onPageCommitted(page.serverTimestamp)

    lastServerTimestamp = page.serverTimestamp
    cursor = page.serverTimestamp
    cursorTableName = page.lastTableName
    cursorRowPks = page.lastRowPks
  }
  } finally {
    // Surface the aggregate ONCE — normal exit, `break` (stall guard), or a
    // throw from `applyPageAsync`. `surfaceRejectedBatch` no-ops on 0, so
    // clean pulls stay silent.
    surfaceRejectedBatch(spaceId, totalRejected)
  }

  return { totalApplied, pageCount, tablesAffected, maxHlc, lastServerTimestamp }
}
