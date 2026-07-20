import { and, eq, inArray } from 'drizzle-orm'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('~/stores/vault', () => ({
  requireDb: vi.fn(),
}))

import {
  handleBookmarksCollectionCreateAsync,
  handleBookmarksCollectionsListAsync,
  handleBookmarksDeleteAsync,
  handleBookmarksDeviceUpsertAsync,
  handleBookmarksListAsync,
  handleBookmarksUpsertAsync,
} from '~/composables/handlers/useCoreExternalRequestHandlers/bookmarks'
import type { ExternalCoreRequest } from '~/composables/handlers/useCoreExternalRequestHandlers/types'
import { haexBookmarkCollections, haexBookmarkDevices, haexBookmarks } from '~/database/schemas/bookmarks'
import { requireDb } from '~/stores/vault'
import { createMockDb, queryResult, type MockDb } from './testDb'

const COLLECTION_ID = '11111111-1111-1111-1111-111111111111'
const OTHER_COLLECTION_ID = '99999999-9999-9999-9999-999999999999'
const NODE_ID = '22222222-2222-2222-2222-222222222222'

function makeRequest(action: string, payload: Record<string, unknown>): ExternalCoreRequest {
  return {
    requestId: 'req-1',
    publicKey: 'device-key',
    action,
    payload,
    extensionPublicKey: 'ext-key',
    extensionName: 'Test Extension',
  }
}

let mockDb: MockDb

beforeEach(() => {
  vi.clearAllMocks()
  mockDb = createMockDb()
  vi.mocked(requireDb).mockReturnValue(mockDb as unknown as ReturnType<typeof requireDb>)
})

describe('handleBookmarksCollectionsListAsync', () => {
  it('returns bookmark counts and device labels per collection', async () => {
    mockDb.select
      .mockReturnValueOnce(queryResult([
        { id: COLLECTION_ID, name: 'Privat', createdAt: 't0', updatedAt: 't1' },
      ]))
      .mockReturnValueOnce(queryResult([{ count: 3 }]))
      .mockReturnValueOnce(queryResult([{ deviceLabel: 'Firefox' }, { deviceLabel: 'Chrome' }]))

    const response = await handleBookmarksCollectionsListAsync(makeRequest('bookmarks-collections-list', {}))

    expect(response.success).toBe(true)
    expect(response.data).toEqual({
      collections: [
        {
          id: COLLECTION_ID,
          name: 'Privat',
          updatedAt: 't1',
          bookmarkCount: 3,
          deviceLabels: ['Firefox', 'Chrome'],
        },
      ],
    })
  })
})

describe('handleBookmarksCollectionCreateAsync', () => {
  it('creates a new collection and returns its id', async () => {
    mockDb.insert.mockReturnValue(queryResult(undefined))

    const response = await handleBookmarksCollectionCreateAsync(
      makeRequest('bookmarks-collection-create', { name: 'Arbeit' }),
    )

    expect(response.success).toBe(true)
    expect(mockDb.insert).toHaveBeenCalledWith(haexBookmarkCollections)
    expect(typeof (response.data as { collectionId: string }).collectionId).toBe('string')
  })

  it('rejects an empty name', async () => {
    const response = await handleBookmarksCollectionCreateAsync(
      makeRequest('bookmarks-collection-create', { name: '' }),
    )

    expect(response.success).toBe(false)
    expect(mockDb.insert).not.toHaveBeenCalled()
  })
})

describe('handleBookmarksListAsync', () => {
  it('returns COLLECTION_NOT_FOUND when the collection does not exist', async () => {
    mockDb.select.mockReturnValueOnce(queryResult([]))

    const response = await handleBookmarksListAsync(
      makeRequest('bookmarks-list', { collectionId: COLLECTION_ID }),
    )

    expect(response.success).toBe(false)
    expect(response.error).toBe('COLLECTION_NOT_FOUND')
  })

  it('returns only the requested collection nodes without CRDT-internal columns', async () => {
    const nodeRow = {
      id: NODE_ID,
      collectionId: COLLECTION_ID,
      parentId: null,
      rootKind: 'toolbar',
      kind: 'folder',
      title: 'Toolbar',
      url: null,
      position: 0,
      createdAt: 't0',
      updatedAt: 't0',
    }

    mockDb.select
      .mockReturnValueOnce(queryResult([{ id: COLLECTION_ID }]))
      .mockReturnValueOnce(queryResult([nodeRow]))

    const response = await handleBookmarksListAsync(
      makeRequest('bookmarks-list', { collectionId: COLLECTION_ID }),
    )

    expect(response.success).toBe(true)
    expect(response.data).toEqual({ nodes: [nodeRow] })

    // The projection passed to the second `select()` must not reach for
    // CRDT-internal columns at all.
    const projection = mockDb.select.mock.calls[1]![0] as Record<string, unknown>
    expect(projection).not.toHaveProperty('haexHlc')
    expect(projection).not.toHaveProperty('haexColumnHlcs')
  })
})

describe('handleBookmarksUpsertAsync', () => {
  const node = {
    id: NODE_ID,
    collectionId: COLLECTION_ID,
    parentId: null,
    rootKind: 'toolbar' as const,
    kind: 'folder' as const,
    title: 'Toolbar',
    url: null,
    position: 0,
  }

  it('returns COLLECTION_NOT_FOUND when the collection does not exist', async () => {
    mockDb.select.mockReturnValueOnce(queryResult([]))

    const response = await handleBookmarksUpsertAsync(
      makeRequest('bookmarks-upsert', { collectionId: COLLECTION_ID, nodes: [node] }),
    )

    expect(response.success).toBe(false)
    expect(response.error).toBe('COLLECTION_NOT_FOUND')
  })

  it('rejects a batch containing a node from another collection', async () => {
    mockDb.select.mockReturnValueOnce(queryResult([{ id: COLLECTION_ID }]))

    const foreignNode = { ...node, collectionId: OTHER_COLLECTION_ID }
    const response = await handleBookmarksUpsertAsync(
      makeRequest('bookmarks-upsert', { collectionId: COLLECTION_ID, nodes: [foreignNode] }),
    )

    expect(response.success).toBe(false)
    expect(response.error).toBe('INVALID_BOOKMARK_NODE')
    expect(mockDb.insert).not.toHaveBeenCalled()
    expect(mockDb.update).not.toHaveBeenCalled()
  })

  it('inserts a node that does not exist yet', async () => {
    mockDb.select
      .mockReturnValueOnce(queryResult([{ id: COLLECTION_ID }]))
      .mockReturnValueOnce(queryResult([]))
    mockDb.insert.mockReturnValue(queryResult(undefined))

    const response = await handleBookmarksUpsertAsync(
      makeRequest('bookmarks-upsert', { collectionId: COLLECTION_ID, nodes: [node] }),
    )

    expect(response.success).toBe(true)
    expect(mockDb.insert).toHaveBeenCalledWith(haexBookmarks)
    expect(mockDb.update).not.toHaveBeenCalled()
  })

  it('updates a node that already exists (idempotent retry)', async () => {
    mockDb.select
      .mockReturnValueOnce(queryResult([{ id: COLLECTION_ID }]))
      .mockReturnValueOnce(queryResult([{ id: NODE_ID }]))
    mockDb.update.mockReturnValue(queryResult(undefined))

    const response = await handleBookmarksUpsertAsync(
      makeRequest('bookmarks-upsert', { collectionId: COLLECTION_ID, nodes: [node] }),
    )

    expect(response.success).toBe(true)
    expect(mockDb.update).toHaveBeenCalledWith(haexBookmarks)
    expect(mockDb.insert).not.toHaveBeenCalled()
  })
})

describe('handleBookmarksDeleteAsync', () => {
  it('hard-deletes only rows within the requested collection', async () => {
    mockDb.delete.mockReturnValue(queryResult(undefined))

    const response = await handleBookmarksDeleteAsync(
      makeRequest('bookmarks-delete', { collectionId: COLLECTION_ID, ids: [NODE_ID] }),
    )

    expect(response.success).toBe(true)
    expect(mockDb.delete).toHaveBeenCalledWith(haexBookmarks)
    const deleteWhereMock = mockDb.delete.mock.results[0]!.value.where
    expect(deleteWhereMock).toHaveBeenCalledWith(
      and(eq(haexBookmarks.collectionId, COLLECTION_ID), inArray(haexBookmarks.id, [NODE_ID])),
    )
  })

  it('does not touch the database for an empty id list', async () => {
    const response = await handleBookmarksDeleteAsync(
      makeRequest('bookmarks-delete', { collectionId: COLLECTION_ID, ids: [] }),
    )

    expect(response.success).toBe(true)
    expect(mockDb.delete).not.toHaveBeenCalled()
  })
})

describe('handleBookmarksDeviceUpsertAsync', () => {
  const payload = {
    collectionId: COLLECTION_ID,
    replicaId: 'replica-1',
    deviceLabel: 'Firefox on laptop',
    browserFamily: 'firefox',
  }

  it('rejects a device label that is too long without touching the database', async () => {
    const response = await handleBookmarksDeviceUpsertAsync(
      makeRequest('bookmarks-device-upsert', { ...payload, deviceLabel: 'a'.repeat(81) }),
    )

    expect(response.success).toBe(false)
    expect(requireDb).not.toHaveBeenCalled()
  })

  it('returns COLLECTION_NOT_FOUND when the collection does not exist', async () => {
    mockDb.select.mockReturnValueOnce(queryResult([]))

    const response = await handleBookmarksDeviceUpsertAsync(
      makeRequest('bookmarks-device-upsert', payload),
    )

    expect(response.success).toBe(false)
    expect(response.error).toBe('COLLECTION_NOT_FOUND')
  })

  it('inserts a new device registry row', async () => {
    mockDb.select
      .mockReturnValueOnce(queryResult([{ id: COLLECTION_ID }]))
      .mockReturnValueOnce(queryResult([]))
    mockDb.insert.mockReturnValue(queryResult(undefined))

    const response = await handleBookmarksDeviceUpsertAsync(
      makeRequest('bookmarks-device-upsert', payload),
    )

    expect(response.success).toBe(true)
    expect(mockDb.insert).toHaveBeenCalledWith(haexBookmarkDevices)
  })

  it('updates an existing device registry row', async () => {
    mockDb.select
      .mockReturnValueOnce(queryResult([{ id: COLLECTION_ID }]))
      .mockReturnValueOnce(queryResult([{ id: 'device-1' }]))
    mockDb.update.mockReturnValue(queryResult(undefined))

    const response = await handleBookmarksDeviceUpsertAsync(
      makeRequest('bookmarks-device-upsert', payload),
    )

    expect(response.success).toBe(true)
    expect(mockDb.update).toHaveBeenCalledWith(haexBookmarkDevices)
    expect(mockDb.insert).not.toHaveBeenCalled()
  })
})
