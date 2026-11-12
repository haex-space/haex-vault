// What a `search` control of the header does with the text the user types (spec 090-app-header-bar,
// FR-012, FR-013, research R7). Pure and clock-free: the caller hands in `schedule`, so the Node
// check script (`scripts/check-wm-header.ts`) drives it with a fake one.

/** Changes of the text within this many ms of the first one give one `input` event. */
export const INPUT_MERGE_MS = 100

export type HeaderSearchDeps = {
  /** The merged text, after a quiet moment or at once on Enter, blur and a new description. */
  emitInput(text: string): void
  /** Enter. */
  emitSubmit(text: string): void
  /** Runs `fn` after `ms`; the returned function cancels it. */
  schedule(fn: () => void, ms: number): () => void
}

export type HeaderSearch = {
  /** What the field shows. */
  text(): string
  /** The user typed. */
  type(text: string): void
  enter(): void
  focus(): void
  blur(): void
  /** A new description arrived with this `value` for the control. */
  describe(value: string): void
}

/**
 * One search field. The app is told about the typed text through merged `input` events; its own
 * `value` in a later description does not overwrite what the user typed:
 *
 * - a `value` equal to the previous description's `value` changes nothing (the app did not change
 *   its mind, whatever is typed);
 * - a changed `value` is applied when the field has no focus; while it has focus it waits, because
 *   the echo of an older input can arrive after the user typed on, and is applied on blur if it
 *   differs from the text then.
 */
export function createHeaderSearch(
  initialValue: string,
  deps: HeaderSearchDeps,
): HeaderSearch {
  let text = initialValue
  let described = initialValue
  let focused = false
  let waiting: string | null = null
  let received: string | null = null
  let cancel: (() => void) | null = null

  function flush() {
    cancel?.()
    cancel = null
    if (waiting === null) return
    const merged = waiting
    waiting = null
    deps.emitInput(merged)
  }

  return {
    text: () => text,
    type(next) {
      text = next
      waiting = next
      cancel ??= deps.schedule(() => {
        cancel = null
        flush()
      }, INPUT_MERGE_MS)
    },
    enter() {
      flush()
      deps.emitSubmit(text)
    },
    focus() {
      focused = true
    },
    blur() {
      flush()
      focused = false
      if (received !== null && received !== text) text = received
      received = null
    },
    describe(value) {
      flush()
      if (value === described) return
      described = value
      if (focused) received = value
      else text = value
    },
  }
}
