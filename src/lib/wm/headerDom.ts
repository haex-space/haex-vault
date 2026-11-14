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

type Focusable = { focus(): void }

const WINDOW = '[data-wm-window-id]'
const ROW = (testId: string) =>
  `[data-testid="wm-header"] [data-testid="${testId}"]`
const PANEL = (testId: string) =>
  `[data-testid="wm-header-overflow-menu"] [data-testid="${testId}"]`
const OVERFLOW_BUTTON =
  '[data-testid="wm-header"] [data-testid="wm-header-overflow"]'

const inWindow = (from: HeaderSource | null | undefined, selector: string) =>
  from?.closest(WINDOW)?.querySelector(selector) ?? null

/** The element of the header's control with this test id, seen from any element of the same window. */
export function headerControl<T extends Focusable = Focusable>(
  from: HeaderSource | null | undefined,
  testId: string,
): T | null {
  return inWindow(from, ROW(testId)) as T | null
}

/**
 * Focuses the header control `testId` of the window `from` sits in. A control that has collapsed
 * is not in the row but in the overflow panel, which draws it only while it is open: then the
 * panel is opened first (`openPanel`) and the control focused once it is drawn (`afterRender`).
 * Whether there is a control to focus; `false` when it is in neither place (the app left it out of
 * the header, for instance while an entry is open).
 */
export function focusHeaderControl(
  from: HeaderSource | null | undefined,
  testId: string,
  openPanel: (() => void) | null,
  afterRender: () => Promise<void>,
): boolean {
  const inRow = inWindow(from, ROW(testId)) as Focusable | null
  if (inRow) {
    inRow.focus()
    return true
  }
  if (!openPanel || !inWindow(from, OVERFLOW_BUTTON)) return false
  openPanel()
  void afterRender().then(() =>
    (inWindow(from, PANEL(testId)) as Focusable | null)?.focus(),
  )
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
