// Selection, holzi's own clipboard and what a paste or drop may do (spec 044 FR-018, FR-022, T049).
// Pure, so `scripts/check-files-clipboard.ts` runs it without vue. The clipboard lives in holzi only;
// the system's clipboard never sees paths.
import { separatorOf } from './state.ts'

/** The selected entries of a folder (by path) and where a Shift click ranges from. */
export type Selection = { paths: string[]; anchor: string | null }

export const EMPTY_SELECTION: Selection = { paths: [], anchor: null }

/** Keys held during a click: toggle (Ctrl or Cmd), range (Shift); `touch` for a tap. */
export type ClickKeys = { toggle: boolean; range: boolean; touch: boolean }

/**
 * What a click on `path` does. A plain mouse click selects the entry alone (a double click opens
 * it, `FolderView.vue`). A tap opens the entry while nothing is selected and toggles it otherwise,
 * as there are no keys on touch. Toggle adds or removes, range selects from the anchor to `path` in
 * the folder's `order`.
 */
export function clickEntry(
  selection: Selection,
  path: string,
  order: readonly string[],
  keys: ClickKeys,
): { selection: Selection; activate: boolean } {
  if (keys.range && selection.anchor !== null) {
    const from = order.indexOf(selection.anchor)
    const to = order.indexOf(path)
    if (from >= 0 && to >= 0) {
      const [start, end] = from <= to ? [from, to] : [to, from]
      return {
        selection: {
          paths: order.slice(start, end + 1),
          anchor: selection.anchor,
        },
        activate: false,
      }
    }
  }
  if (!keys.toggle && !keys.range) {
    if (!keys.touch) {
      return { selection: { paths: [path], anchor: path }, activate: false }
    }
    if (selection.paths.length === 0) return { selection, activate: true }
  }
  const paths = selection.paths.includes(path)
    ? selection.paths.filter((p) => p !== path)
    : [...selection.paths, path]
  return {
    selection: { paths, anchor: paths.length ? path : null },
    activate: false,
  }
}

/** A rectangle dragged with the mouse, in the scrolled content's pixels. */
export type Band = { left: number; top: number; right: number; bottom: number }

/** How the entries lie in the content: `columns` per row, every cell `cellWidth` x `cellHeight`
 * (the list is one column as wide as it gets). */
export type CellGrid = {
  columns: number
  cellWidth: number
  cellHeight: number
}

/** The band between two points, whichever corner it was dragged from. */
export function bandBetween(
  from: { x: number; y: number },
  to: { x: number; y: number },
): Band {
  return {
    left: Math.min(from.x, to.x),
    top: Math.min(from.y, to.y),
    right: Math.max(from.x, to.x),
    bottom: Math.max(from.y, to.y),
  }
}

/**
 * The indexes (in folder order) of the entries a band touches. A band over the empty area right of
 * the last column, or below the last row, touches nothing there.
 */
export function indexesInBand(
  band: Band,
  grid: CellGrid,
  count: number,
): number[] {
  if (count <= 0 || band.bottom < 0 || band.right < 0) return []
  if (band.left >= grid.columns * grid.cellWidth) return []
  const firstColumn = Math.max(0, Math.floor(band.left / grid.cellWidth))
  const lastColumn = Math.min(
    grid.columns - 1,
    Math.floor(band.right / grid.cellWidth),
  )
  const firstRow = Math.max(0, Math.floor(band.top / grid.cellHeight))
  const lastRow = Math.floor(band.bottom / grid.cellHeight)
  const out: number[] = []
  for (let row = firstRow; row <= lastRow; row++) {
    for (let column = firstColumn; column <= lastColumn; column++) {
      const index = row * grid.columns + column
      if (index >= count) return out
      out.push(index)
    }
  }
  return out
}

/**
 * The selection while a band is dragged: what it touches. With Ctrl, Cmd or Shift it adds to what
 * was selected when the band started (`base`), else it replaces it.
 */
export function bandSelection(
  base: Selection,
  touched: readonly string[],
  additive: boolean,
): Selection {
  const paths = additive
    ? [...new Set([...base.paths, ...touched])]
    : [...touched]
  const anchor = additive
    ? (base.anchor ?? paths[0] ?? null)
    : (paths[0] ?? null)
  return { paths, anchor }
}

/** The selection a context menu on `path` acts on: the selection when it holds the entry, else the
 * entry alone. */
export function menuSelection(selection: Selection, path: string): Selection {
  return selection.paths.includes(path)
    ? selection
    : { paths: [path], anchor: path }
}

/** Keeps only paths that are still in the folder (after a reload). */
export function pruneSelection(
  selection: Selection,
  present: readonly string[],
): Selection {
  const paths = selection.paths.filter((path) => present.includes(path))
  if (paths.length === selection.paths.length) return selection
  return {
    paths,
    anchor:
      selection.anchor !== null && paths.includes(selection.anchor)
        ? selection.anchor
        : null,
  }
}

/** One copied or cut entry. */
export type ClipboardItem = { path: string; kind: 'file' | 'dir' }

/** holzi's clipboard: entries of one folder of one source, to copy or to move. */
export type FilesClipboard = {
  op: 'copy' | 'cut'
  source: { kind: string; storageId?: string }
  folder: string
  items: ClipboardItem[]
}

/** Whether two source references name the same source. */
export function sameSource(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b)
}

/** Whether `inner` is `outer` or lies below it. */
export function isWithin(inner: string, outer: string): boolean {
  if (inner === outer) return true
  const sep = separatorOf(outer)
  const prefix = outer.endsWith(sep) ? outer : `${outer}${sep}`
  return inner.startsWith(prefix)
}

/** Why a folder takes no changes: it is one of holzi's own places (`holziOwned`), or a share read only. */
export type PlaceLock = { holziOwned: boolean; readOnly: boolean }

export const isLocked = (lock: PlaceLock | null | undefined): boolean =>
  !!lock && (lock.holziOwned || lock.readOnly)

/** Why entries cannot go into a folder, or `null` when they can. `nothing`: cut and pasted into the
 * folder they are in, so there is nothing to do. */
export type PasteRefusal = 'intoItself' | 'holziOwned' | 'readOnly' | 'nothing'

/**
 * Whether `items` from `folder` may be copied (`copy`) or moved (`cut`) into `target`. The paths of
 * two sources are separate namespaces: with `sameSource` false (a storage and another storage, the
 * device and a storage) `/backup` going into `/backup/2026` is not "into itself" and a cut from `/`
 * to `/` is not "the same folder", so only the rule of holzi's own places applies (spec 062).
 */
export function pasteRefusal(
  op: FilesClipboard['op'],
  folder: string,
  items: readonly ClipboardItem[],
  target: { path: string; holziOwned: boolean; readOnly?: boolean },
  sameSource = true,
): PasteRefusal | null {
  if (target.holziOwned) return 'holziOwned'
  if (target.readOnly) return 'readOnly'
  if (!sameSource) return null
  if (
    items.some(
      (item) => item.kind === 'dir' && isWithin(target.path, item.path),
    )
  )
    return 'intoItself'
  if (op === 'cut' && folder === target.path) return 'nothing'
  return null
}

/** What dragging onto a folder does: move by default, copy with Ctrl (Alt on macOS), as in the
 * system's file manager; between sources always copy. */
export function dropOp(
  sameSource: boolean,
  keys: { ctrl: boolean; alt: boolean; mac: boolean },
): 'copy' | 'cut' {
  if (!sameSource) return 'copy'
  return (keys.mac ? keys.alt : keys.ctrl) ? 'copy' : 'cut'
}

/** The MIME type of entries dragged inside holzi. */
export const ENTRIES_MIME = 'application/x-holzi-files-entries'

/** What a drag inside holzi carries: the source, the folder and the entries. */
export type DragPayload = Omit<FilesClipboard, 'op'>

export function dragPayload(payload: DragPayload): string {
  return JSON.stringify(payload)
}

/** The entries of a drop, or `null` when the text is not ours. */
export function parseDragPayload(
  text: string | null | undefined,
): DragPayload | null {
  if (!text) return null
  try {
    const parsed = JSON.parse(text) as Partial<DragPayload>
    const items = parsed.items
    if (
      typeof parsed.folder !== 'string' ||
      typeof parsed.source?.kind !== 'string' ||
      !Array.isArray(items) ||
      items.length === 0 ||
      !items.every(
        (item) =>
          typeof item?.path === 'string' &&
          (item.kind === 'file' || item.kind === 'dir'),
      )
    )
      return null
    return {
      source: parsed.source,
      folder: parsed.folder,
      items,
    }
  } catch {
    return null
  }
}
