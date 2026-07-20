import { and, eq, inArray, sql } from 'drizzle-orm'
import {
  haexBookmarkCollections,
  haexBookmarkDevices,
  haexBookmarks,
} from '~/database/schemas/bookmarks'
import { requireDb } from '~/stores/vault'
import {
  BookmarkValidationError,
  validateCollectionName,
  validateDeviceLabel,
  validateUpsertBatch,
} from '~/utils/bookmarks/validation'
import { errorResponse } from './shared'
import type {
  BookmarksCollectionCreatePayload,
  BookmarksDeletePayload,
  BookmarksDeviceUpsertPayload,
  BookmarksListPayload,
  BookmarksUpsertPayload,
  ExternalCoreRequest,
  ExternalCoreResponse,
} from './types'

export const COLLECTION_NOT_FOUND = 'COLLECTION_NOT_FOUND'
export const INVALID_BOOKMARK_NODE = 'INVALID_BOOKMARK_NODE'

type Db = ReturnType<typeof requireDb>

const collectionExistsAsync = async (db: Db, collectionId: string): Promise<boolean> => {
  const [existing] = await db
    .select({ id: haexBookmarkCollections.id })
    .from(haexBookmarkCollections)
    .where(eq(haexBookmarkCollections.id, collectionId))
    .limit(1)
  return existing !== undefined
}

// ---------------------------------------------------------------------------
// bookmarks-collections-list
// ---------------------------------------------------------------------------

export const handleBookmarksCollectionsListAsync = async (
  request: ExternalCoreRequest,
): Promise<ExternalCoreResponse> => {
  const db = requireDb()

  const collections = await db.select().from(haexBookmarkCollections)
  const collectionIds = collections.map((collection) => collection.id)

  const bookmarkRows = collectionIds.length
    ? await db
        .select({ collectionId: haexBookmarks.collectionId })
        .from(haexBookmarks)
        .where(inArray(haexBookmarks.collectionId, collectionIds))
    : []
  const countsByCollection = new Map<string, number>()
  for (const row of bookmarkRows) {
    countsByCollection.set(row.collectionId, (countsByCollection.get(row.collectionId) ?? 0) + 1)
  }

  const deviceRows = collectionIds.length
    ? await db
        .select({
          collectionId: haexBookmarkDevices.collectionId,
          deviceLabel: haexBookmarkDevices.deviceLabel,
        })
        .from(haexBookmarkDevices)
        .where(inArray(haexBookmarkDevices.collectionId, collectionIds))
    : []
  const deviceLabelsByCollection = new Map<string, string[]>()
  for (const row of deviceRows) {
    const labels = deviceLabelsByCollection.get(row.collectionId) ?? []
    labels.push(row.deviceLabel)
    deviceLabelsByCollection.set(row.collectionId, labels)
  }

  const data = collections.map((collection) => ({
    id: collection.id,
    name: collection.name,
    updatedAt: collection.updatedAt,
    bookmarkCount: countsByCollection.get(collection.id) ?? 0,
    deviceLabels: deviceLabelsByCollection.get(collection.id) ?? [],
  }))

  return { requestId: request.requestId, success: true, data: { collections: data } }
}

// ---------------------------------------------------------------------------
// bookmarks-collection-create
// ---------------------------------------------------------------------------

export const handleBookmarksCollectionCreateAsync = async (
  request: ExternalCoreRequest,
): Promise<ExternalCoreResponse> => {
  const { name } = request.payload as BookmarksCollectionCreatePayload
  if (!name) return errorResponse(request.requestId, 'Missing required field: name')

  try {
    validateCollectionName(name)
  } catch (error) {
    if (error instanceof BookmarkValidationError) return errorResponse(request.requestId, error.message)
    throw error
  }

  const db = requireDb()
  const collectionId = crypto.randomUUID()
  await db.insert(haexBookmarkCollections).values({ id: collectionId, name })

  return { requestId: request.requestId, success: true, data: { collectionId } }
}

// ---------------------------------------------------------------------------
// bookmarks-list
// ---------------------------------------------------------------------------

export const handleBookmarksListAsync = async (
  request: ExternalCoreRequest,
): Promise<ExternalCoreResponse> => {
  const { collectionId } = request.payload as BookmarksListPayload
  if (!collectionId) return errorResponse(request.requestId, 'Missing required field: collectionId')

  const db = requireDb()

  if (!(await collectionExistsAsync(db, collectionId))) {
    return errorResponse(request.requestId, COLLECTION_NOT_FOUND)
  }

  const nodes = await db
    .select({
      id: haexBookmarks.id,
      collectionId: haexBookmarks.collectionId,
      parentId: haexBookmarks.parentId,
      rootKind: haexBookmarks.rootKind,
      kind: haexBookmarks.kind,
      title: haexBookmarks.title,
      url: haexBookmarks.url,
      position: haexBookmarks.position,
      createdAt: haexBookmarks.createdAt,
      updatedAt: haexBookmarks.updatedAt,
    })
    .from(haexBookmarks)
    .where(eq(haexBookmarks.collectionId, collectionId))

  return { requestId: request.requestId, success: true, data: { nodes } }
}

// ---------------------------------------------------------------------------
// bookmarks-upsert
// ---------------------------------------------------------------------------

export const handleBookmarksUpsertAsync = async (
  request: ExternalCoreRequest,
): Promise<ExternalCoreResponse> => {
  const { collectionId, nodes } = request.payload as BookmarksUpsertPayload
  if (!collectionId) return errorResponse(request.requestId, 'Missing required field: collectionId')
  if (!nodes) return errorResponse(request.requestId, 'Missing required field: nodes')

  const db = requireDb()

  if (!(await collectionExistsAsync(db, collectionId))) {
    return errorResponse(request.requestId, COLLECTION_NOT_FOUND)
  }

  try {
    validateUpsertBatch(collectionId, nodes)
  } catch (error) {
    if (error instanceof BookmarkValidationError) return errorResponse(request.requestId, INVALID_BOOKMARK_NODE)
    throw error
  }

  // A single batched existence lookup replaces one `select` per node — the
  // driver has no `onConflictDoUpdate`/`batch()` support (see PR description),
  // so this is the update/insert split with the fewest round-trips available.
  const existingIds = new Set(
    (
      await db
        .select({ id: haexBookmarks.id })
        .from(haexBookmarks)
        .where(
          inArray(
            haexBookmarks.id,
            nodes.map((node) => node.id),
          ),
        )
    ).map((row) => row.id),
  )

  for (const node of nodes) {
    const values = {
      collectionId: node.collectionId,
      parentId: node.parentId,
      rootKind: node.rootKind,
      kind: node.kind,
      title: node.title,
      url: node.url,
      position: node.position,
    }

    if (existingIds.has(node.id)) {
      await db.update(haexBookmarks).set(values).where(eq(haexBookmarks.id, node.id))
    } else {
      await db.insert(haexBookmarks).values({ id: node.id, ...values })
    }
  }

  return { requestId: request.requestId, success: true, data: { upserted: nodes.length } }
}

// ---------------------------------------------------------------------------
// bookmarks-delete
// ---------------------------------------------------------------------------

export const handleBookmarksDeleteAsync = async (
  request: ExternalCoreRequest,
): Promise<ExternalCoreResponse> => {
  const { collectionId, ids } = request.payload as BookmarksDeletePayload
  if (!collectionId) return errorResponse(request.requestId, 'Missing required field: collectionId')
  if (!ids) return errorResponse(request.requestId, 'Missing required field: ids')

  const db = requireDb()

  // Scoped to collectionId so a delete in one collection can never remove
  // rows from another, even if the caller sends a foreign id by mistake.
  if (ids.length > 0) {
    await db
      .delete(haexBookmarks)
      .where(and(eq(haexBookmarks.collectionId, collectionId), inArray(haexBookmarks.id, ids)))
  }

  return { requestId: request.requestId, success: true, data: { deleted: ids.length } }
}

// ---------------------------------------------------------------------------
// bookmarks-device-upsert
// ---------------------------------------------------------------------------

export const handleBookmarksDeviceUpsertAsync = async (
  request: ExternalCoreRequest,
): Promise<ExternalCoreResponse> => {
  const { collectionId, replicaId, deviceLabel, browserFamily } =
    request.payload as BookmarksDeviceUpsertPayload
  if (!collectionId) return errorResponse(request.requestId, 'Missing required field: collectionId')
  if (!replicaId) return errorResponse(request.requestId, 'Missing required field: replicaId')
  if (!deviceLabel) return errorResponse(request.requestId, 'Missing required field: deviceLabel')
  if (!browserFamily) return errorResponse(request.requestId, 'Missing required field: browserFamily')

  try {
    validateDeviceLabel(deviceLabel)
  } catch (error) {
    if (error instanceof BookmarkValidationError) return errorResponse(request.requestId, error.message)
    throw error
  }

  const db = requireDb()

  if (!(await collectionExistsAsync(db, collectionId))) {
    return errorResponse(request.requestId, COLLECTION_NOT_FOUND)
  }

  const [existing] = await db
    .select({ id: haexBookmarkDevices.id })
    .from(haexBookmarkDevices)
    .where(
      and(
        eq(haexBookmarkDevices.collectionId, collectionId),
        eq(haexBookmarkDevices.replicaId, replicaId),
      ),
    )
    .limit(1)

  if (existing) {
    await db
      .update(haexBookmarkDevices)
      .set({ deviceLabel, browserFamily, lastSeenAt: sql`(CURRENT_TIMESTAMP)` })
      .where(eq(haexBookmarkDevices.id, existing.id))
  } else {
    await db.insert(haexBookmarkDevices).values({
      id: crypto.randomUUID(),
      collectionId,
      replicaId,
      deviceLabel,
      browserFamily,
    })
  }

  return { requestId: request.requestId, success: true, data: {} }
}
