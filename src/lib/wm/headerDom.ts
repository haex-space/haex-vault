// Where a control of the window's header is in the page, for an app that has to hand the focus to it
// (spec 093-app-headers, FR-004 and the "Keyboard" edge case). The header belongs to the window, so it
// sits outside the app's own root: look from the app's element up to its window, then down. Only
// `closest` and `querySelector` are used, so the Node check (`scripts/check-passwords-header.ts`)
// runs it on stubs.

/** The part of a DOM element these helpers read (an `Element` fits; the Node checks pass stubs). */
export type HeaderKeyTarget = {
  closest(selector: string): HeaderKeyTarget | null
  getAttribute(name: string): string | null
}

/** Where a lookup starts: any element (`Element` fits). */
export type HeaderSource = {
  closest(selector: string): HeaderSource | null
  querySelector(selector: string): unknown
}

const WINDOW = '[data-wm-window-id]'
const ROW = (testId: string) =>
  `[data-testid="wm-header"] [data-testid="${testId}"]`
const FIELD = '[data-testid="wm-header-search-popover"] input'
const OVERFLOW_BUTTON =
  '[data-testid="wm-header"] [data-testid="wm-header-overflow"]'

const inWindow = (from: HeaderSource | null | undefined, selector: string) =>
  from?.closest(WINDOW)?.querySelector(selector) ?? null

/**
 * Opens the field of the header search `testId` of the window `from` sits in. The search is a button
 * of the header, in the row or, collapsed, in the overflow sidebar that the overflow toggle opens;
 * either way its field opens under the header (`open` sets what the window shows) and takes the
 * focus. Whether there is a search to open: `false` without a window manager (`open` is `null`) and
 * when it is in neither place (the app left it out of the header, for instance while an entry is
 * open).
 */
export function openHeaderSearch(
  from: HeaderSource | null | undefined,
  testId: string,
  open: (() => void) | null,
): boolean {
  if (!open) return false
  if (!inWindow(from, ROW(testId)) && !inWindow(from, OVERFLOW_BUTTON))
    return false
  open()
  // A field that is already open keeps its id, so nothing reacts to `open`: the focus is given here.
  const field = inWindow(from, FIELD) as { focus(): void } | null
  field?.focus()
  return true
}

/**
 * Whether a key event's target is a control of the header of window `windowId`. The header is drawn
 * by the window, outside an app's own root, so its keys do not reach the app's keyboard handler by
 * bubbling; the app listens for them itself and uses this to take only its own window's (several
 * windows and tabs are mounted at once).
 */
export function inWindowHeader(
  target: HeaderKeyTarget | null,
  windowId: string,
): boolean {
  const owner = target?.closest('[data-testid="wm-header"]')?.closest(WINDOW)
  return !!owner && owner.getAttribute('data-wm-window-id') === windowId
}
