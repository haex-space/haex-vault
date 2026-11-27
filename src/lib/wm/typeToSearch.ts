// Type to search: a printable key pressed while no field has the focus opens the window's header
// search with that character in it. Whether a key event counts is decided here, without the DOM,
// so the Node check (`scripts/check-wm-type-to-search.ts`) runs it on stubs.

/** The part of a DOM element the editable test reads (an `Element` fits). */
export type TypeTarget = { closest(selector: string): unknown } | null

/** The part of a `KeyboardEvent` that decides (a `KeyboardEvent` fits). */
export type TypeKeyEvent = {
  key: string
  ctrlKey: boolean
  metaKey: boolean
  altKey: boolean
  isComposing: boolean
  defaultPrevented: boolean
  target: unknown
}

/** What takes typed text itself, so the key is its own. */
const EDITABLE =
  'input, textarea, select, [contenteditable]:not([contenteditable="false"]), [role="textbox"], [role="combobox"], [role="searchbox"]'

/**
 * Whether `event` is a character the user typed into nothing: a single printable character (a
 * space stays a button's or a list's own key) without Ctrl, Meta or Alt (shortcuts), outside a
 * composition, not already taken by a handler, and not aimed at a field. `ownsTarget` says the
 * key is aimed at this window, or at the page because nothing has the focus.
 */
export function startsTyping(
  event: TypeKeyEvent,
  ownsTarget: boolean,
): boolean {
  if (!ownsTarget || event.defaultPrevented || event.isComposing) return false
  if (event.ctrlKey || event.metaKey || event.altKey) return false
  if (event.key.length !== 1 || event.key.trim() === '') return false
  const target = event.target as TypeTarget
  return !target?.closest?.(EDITABLE)
}
