// Geometry math for the Workspace-Shell (spec 015-workspace-shell, T027): clamping, cascade
// placement, and maximize/compact display resolution. Pure functions — no Nuxt auto-imports,
// relative sibling imports with an explicit .ts extension (plan research R6).
import type { WmWindow, Size } from './types.ts'

type Rect = { x: number; y: number; width: number; height: number }

/** New windows cascade by this offset per step (haex-vault 2.x (Legacy) reference, research R12), wrapping
 * once they would run off the visible area. */
const CASCADE_STEP = 24
const CASCADE_WRAP_MARGIN = 160

/** How much of a dragged window's title bar must stay reachable inside the visible area
 * (FR-009/FR-026, research R7) — deliberately smaller than a real title bar so a window can be
 * pushed mostly off-screen, not just nudged back fully inside it. */
export const MIN_VISIBLE_TITLEBAR: Size = { width: 64, height: 32 }

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), Math.max(min, max))
}

/** Full-clamp for restored geometry (FR-026): the window must end up entirely inside `area`. Used
 * by `layoutState.ts`'s `hydrate` — there is no previous on-screen position to preserve partially,
 * unlike `clampDragPosition`. */
export function clampGeometry(window: Rect, minSize: Size, area: Size): Rect {
  const width = clamp(
    window.width,
    minSize.width,
    Math.max(area.width, minSize.width),
  )
  const height = clamp(
    window.height,
    minSize.height,
    Math.max(area.height, minSize.height),
  )
  const x = clamp(window.x, 0, Math.max(area.width - width, 0))
  const y = clamp(window.y, 0, Math.max(area.height - height, 0))
  return { x, y, width, height }
}

/** Clamps a size to its app's minimum, without touching position — used while resizing (T028). */
export function clampResizeSize(size: Size, minSize: Size): Size {
  return {
    width: Math.max(size.width, minSize.width),
    height: Math.max(size.height, minSize.height),
  }
}

/** Clamps a dragged/resized position so at least `MIN_VISIBLE_TITLEBAR` of the window's top-left
 * corner stays inside `area` (FR-009's "always reachable") — deliberately more permissive than
 * `clampGeometry`'s full containment, since the user is actively controlling the position and may
 * legitimately want most of the window off-screen. */
export function clampDragPosition(
  position: { x: number; y: number },
  size: Size,
  area: Size,
): {
  x: number
  y: number
} {
  const minX = MIN_VISIBLE_TITLEBAR.width - size.width
  const maxX = area.width - MIN_VISIBLE_TITLEBAR.width
  const minY = 0
  const maxY = area.height - MIN_VISIBLE_TITLEBAR.height
  return {
    x: clamp(position.x, minX, Math.max(minX, maxX)),
    y: clamp(position.y, minY, Math.max(minY, maxY)),
  }
}

/** Where the store places a new window if `openApp` does not activate an existing singleton
 * (FR-012). `openInWorkspace` is the number of windows already open in the target workspace. */
export function cascadePosition(
  openInWorkspace: number,
  defaultSize: Size,
  area: Size,
): { x: number; y: number } {
  const offset = (openInWorkspace * CASCADE_STEP) % CASCADE_WRAP_MARGIN
  return {
    x: clamp(offset, 0, Math.max(area.width - defaultSize.width, 0)),
    y: clamp(offset, 0, Math.max(area.height - defaultSize.height, 0)),
  }
}

/**
 * The rect a window actually renders at, as opposed to the rect it stores. Compact mode and
 * maximizing both fill `area` without ever overwriting the window's own normal `x`/`y`/`width`/
 * `height` (research R7: "the normal geometry stays untouched"; FR-028: compact never writes
 * geometry) — restoring is then just clearing a flag, nothing to recompute or remember.
 */
export function windowDisplayRect(
  window: Pick<WmWindow, 'x' | 'y' | 'width' | 'height' | 'maximized'>,
  compact: boolean,
  area: Size,
): Rect {
  if (compact || window.maximized) {
    return { x: 0, y: 0, width: area.width, height: area.height }
  }
  return {
    x: window.x,
    y: window.y,
    width: window.width,
    height: window.height,
  }
}
