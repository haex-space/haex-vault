import { describe, expect, it } from 'vitest'
import {
  BOOKMARK_DEVICE_LABEL_MAX_LENGTH,
  BOOKMARK_TITLE_MAX_LENGTH,
  BOOKMARK_UPSERT_BATCH_LIMIT,
  BOOKMARK_URL_MAX_LENGTH,
  type BookmarkNodeInput,
  validateBookmarkNode,
  validateCollectionName,
  validateDeviceLabel,
  validateUpsertBatch,
} from '~/utils/bookmarks/validation'

const COLLECTION_ID = '11111111-1111-1111-1111-111111111111'
const ID_A = '22222222-2222-2222-2222-222222222222'
const ID_B = '33333333-3333-3333-3333-333333333333'

function makeFolder(overrides: Partial<BookmarkNodeInput> = {}): BookmarkNodeInput {
  return {
    id: ID_A,
    collectionId: COLLECTION_ID,
    parentId: null,
    rootKind: 'toolbar',
    kind: 'folder',
    title: 'Toolbar',
    url: null,
    position: 0,
    ...overrides,
  }
}

function makeBookmark(overrides: Partial<BookmarkNodeInput> = {}): BookmarkNodeInput {
  return {
    id: ID_B,
    collectionId: COLLECTION_ID,
    parentId: ID_A,
    rootKind: null,
    kind: 'bookmark',
    title: 'Example',
    url: 'https://example.com',
    position: 0,
    ...overrides,
  }
}

describe('validateBookmarkNode', () => {
  it('accepts a valid root folder', () => {
    expect(() => validateBookmarkNode(makeFolder())).not.toThrow()
  })

  it('accepts a valid bookmark', () => {
    expect(() => validateBookmarkNode(makeBookmark())).not.toThrow()
  })

  it('accepts a valid separator', () => {
    expect(() => validateBookmarkNode(makeBookmark({ kind: 'separator', url: null, title: null }))).not.toThrow()
  })

  it('rejects a non-UUID id', () => {
    expect(() => validateBookmarkNode(makeBookmark({ id: 'not-a-uuid' }))).toThrow()
  })

  it('rejects a non-UUID parentId', () => {
    expect(() => validateBookmarkNode(makeBookmark({ parentId: 'not-a-uuid' }))).toThrow()
  })

  it('rejects an unknown kind', () => {
    expect(() => validateBookmarkNode(makeBookmark({ kind: 'file' as never }))).toThrow()
  })

  it('rejects a url on a folder', () => {
    expect(() => validateBookmarkNode(makeFolder({ url: 'https://example.com' }))).toThrow()
  })

  it('rejects a url exceeding the max length', () => {
    const longUrl = `https://example.com/${'a'.repeat(BOOKMARK_URL_MAX_LENGTH)}`
    expect(() => validateBookmarkNode(makeBookmark({ url: longUrl }))).toThrow()
  })

  it('rejects rootKind on a non-root node', () => {
    expect(() => validateBookmarkNode(makeBookmark({ rootKind: 'toolbar' }))).toThrow()
  })

  it('rejects a root node without rootKind', () => {
    expect(() => validateBookmarkNode(makeFolder({ rootKind: null }))).toThrow()
  })

  it('rejects rootKind on a non-folder root node', () => {
    expect(() => validateBookmarkNode(makeFolder({ kind: 'separator' }))).toThrow()
  })

  it('rejects an unknown rootKind value', () => {
    expect(() => validateBookmarkNode(makeFolder({ rootKind: 'sidebar' as never }))).toThrow()
  })

  it('rejects a title exceeding the max length', () => {
    expect(() => validateBookmarkNode(makeBookmark({ title: 'a'.repeat(BOOKMARK_TITLE_MAX_LENGTH + 1) }))).toThrow()
  })

  it('rejects control characters in the title', () => {
    expect(() => validateBookmarkNode(makeBookmark({ title: `bad${String.fromCharCode(1)}title` }))).toThrow()
  })

  it('accepts tab/newline/carriage-return in the title', () => {
    expect(() => validateBookmarkNode(makeBookmark({ title: 'line1\nline2\ttab\rcr' }))).not.toThrow()
  })

  it('rejects a negative position', () => {
    expect(() => validateBookmarkNode(makeBookmark({ position: -1 }))).toThrow()
  })

  it('rejects a non-integer position', () => {
    expect(() => validateBookmarkNode(makeBookmark({ position: 1.5 }))).toThrow()
  })
})

describe('validateUpsertBatch', () => {
  it('accepts a valid batch', () => {
    expect(() => validateUpsertBatch(COLLECTION_ID, [makeFolder(), makeBookmark()])).not.toThrow()
  })

  it('rejects a batch exceeding the size limit', () => {
    const nodes = Array.from({ length: BOOKMARK_UPSERT_BATCH_LIMIT + 1 }, (_, i) =>
      makeBookmark({ id: crypto.randomUUID(), parentId: ID_A, position: i }))
    expect(() => validateUpsertBatch(COLLECTION_ID, nodes)).toThrow()
  })

  it('rejects a node whose collectionId does not match the request', () => {
    const foreignFolder = makeFolder({ collectionId: '99999999-9999-9999-9999-999999999999' })
    expect(() => validateUpsertBatch(COLLECTION_ID, [foreignFolder])).toThrow()
  })

  it('rejects duplicate rootKind values within a collection', () => {
    const secondToolbarRoot = makeFolder({ id: crypto.randomUUID() })
    expect(() => validateUpsertBatch(COLLECTION_ID, [makeFolder(), secondToolbarRoot])).toThrow()
  })

  it('allows distinct rootKind values within a collection', () => {
    const menuRoot = makeFolder({ id: crypto.randomUUID(), rootKind: 'menu' })
    expect(() => validateUpsertBatch(COLLECTION_ID, [makeFolder(), menuRoot])).not.toThrow()
  })
})

describe('validateCollectionName', () => {
  it('accepts a non-empty name', () => {
    expect(() => validateCollectionName('Privat')).not.toThrow()
  })

  it('rejects an empty name', () => {
    expect(() => validateCollectionName('')).toThrow()
  })

  it('rejects a whitespace-only name', () => {
    expect(() => validateCollectionName('   ')).toThrow()
  })

  it('rejects control characters in the name', () => {
    expect(() => validateCollectionName(`bad${String.fromCharCode(1)}name`)).toThrow()
  })
})

describe('validateDeviceLabel', () => {
  it('accepts a valid label', () => {
    expect(() => validateDeviceLabel('Firefox on laptop')).not.toThrow()
  })

  it('rejects an empty label', () => {
    expect(() => validateDeviceLabel('')).toThrow()
  })

  it('rejects a label exceeding the max length', () => {
    expect(() => validateDeviceLabel('a'.repeat(BOOKMARK_DEVICE_LABEL_MAX_LENGTH + 1))).toThrow()
  })
})
