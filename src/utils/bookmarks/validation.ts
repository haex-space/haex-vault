const NODE_KINDS = ['folder', 'bookmark', 'separator'] as const
const ROOT_KINDS = ['toolbar', 'menu', 'other', 'mobile'] as const

export type BookmarkNodeKind = (typeof NODE_KINDS)[number]
export type BookmarkRootKind = (typeof ROOT_KINDS)[number]

export interface BookmarkNodeInput {
  id: string
  collectionId: string
  parentId: string | null
  rootKind: BookmarkRootKind | null
  kind: BookmarkNodeKind
  title: string | null
  url: string | null
  position: number
}

export const BOOKMARK_UPSERT_BATCH_LIMIT = 5000
export const BOOKMARK_TITLE_MAX_LENGTH = 4096
export const BOOKMARK_URL_MAX_LENGTH = 65536
export const BOOKMARK_DEVICE_LABEL_MAX_LENGTH = 80

export class BookmarkValidationError extends Error {}

const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
// U+0000-U+001F except tab/LF/CR, which are normalizable whitespace.
const FORBIDDEN_CONTROL_CHARS = new RegExp(
  `[${String.fromCharCode(0)}-${String.fromCharCode(8)}${String.fromCharCode(11)}${String.fromCharCode(12)}${String.fromCharCode(14)}-${String.fromCharCode(31)}]`,
)

function assertUuid(value: string, label: string): void {
  if (!UUID_PATTERN.test(value)) {
    throw new BookmarkValidationError(`${label} must be a UUID`)
  }
}

function assertNoForbiddenControlChars(value: string, label: string): void {
  if (FORBIDDEN_CONTROL_CHARS.test(value)) {
    throw new BookmarkValidationError(`${label} contains forbidden control characters`)
  }
}

export function validateBookmarkNode(node: BookmarkNodeInput): void {
  assertUuid(node.id, 'id')
  if (node.parentId !== null) {
    assertUuid(node.parentId, 'parentId')
  }

  if (!NODE_KINDS.includes(node.kind)) {
    throw new BookmarkValidationError(`kind must be one of ${NODE_KINDS.join(', ')}`)
  }

  if (node.kind === 'bookmark') {
    if (node.url !== null && node.url.length > BOOKMARK_URL_MAX_LENGTH) {
      throw new BookmarkValidationError(`url must not exceed ${BOOKMARK_URL_MAX_LENGTH} characters`)
    }
  }
  else if (node.url !== null) {
    throw new BookmarkValidationError('url is only allowed on kind="bookmark"')
  }

  const isRoot = node.parentId === null
  if (isRoot !== (node.rootKind !== null)) {
    throw new BookmarkValidationError('rootKind must be set if and only if parentId is null')
  }
  if (node.rootKind !== null) {
    if (node.kind !== 'folder') {
      throw new BookmarkValidationError('rootKind is only allowed on kind="folder"')
    }
    if (!ROOT_KINDS.includes(node.rootKind)) {
      throw new BookmarkValidationError(`rootKind must be one of ${ROOT_KINDS.join(', ')}`)
    }
  }

  if (node.title !== null) {
    if (node.title.length > BOOKMARK_TITLE_MAX_LENGTH) {
      throw new BookmarkValidationError(`title must not exceed ${BOOKMARK_TITLE_MAX_LENGTH} characters`)
    }
    assertNoForbiddenControlChars(node.title, 'title')
  }

  if (!Number.isSafeInteger(node.position) || node.position < 0) {
    throw new BookmarkValidationError('position must be a non-negative safe integer')
  }
}

export function validateUpsertBatch(collectionId: string, nodes: BookmarkNodeInput[]): void {
  if (nodes.length > BOOKMARK_UPSERT_BATCH_LIMIT) {
    throw new BookmarkValidationError(`batch must not exceed ${BOOKMARK_UPSERT_BATCH_LIMIT} nodes`)
  }

  const seenRootKinds = new Set<BookmarkRootKind>()
  for (const node of nodes) {
    if (node.collectionId !== collectionId) {
      throw new BookmarkValidationError('node.collectionId must match the request collectionId')
    }
    validateBookmarkNode(node)
    if (node.rootKind !== null) {
      if (seenRootKinds.has(node.rootKind)) {
        throw new BookmarkValidationError(`rootKind "${node.rootKind}" must appear at most once per collection`)
      }
      seenRootKinds.add(node.rootKind)
    }
  }
}

export function validateCollectionName(name: string): void {
  if (name.trim().length === 0) {
    throw new BookmarkValidationError('name must not be empty')
  }
  assertNoForbiddenControlChars(name, 'name')
}

export function validateDeviceLabel(label: string): void {
  if (label.trim().length === 0) {
    throw new BookmarkValidationError('deviceLabel must not be empty')
  }
  if (label.length > BOOKMARK_DEVICE_LABEL_MAX_LENGTH) {
    throw new BookmarkValidationError(`deviceLabel must not exceed ${BOOKMARK_DEVICE_LABEL_MAX_LENGTH} characters`)
  }
  assertNoForbiddenControlChars(label, 'deviceLabel')
}
