import { onScopeDispose, ref, type Ref } from 'vue'
import { createHeaderSearch, type HeaderSearch } from '~/lib/wm/headerSearch'
import type { WmHeaderControl } from '~/types/bindings/WmHeaderControl'
import type { WmHeaderEvent } from '~/types/bindings/WmHeaderEvent'

/** One `search` control as the header's controls use it. */
export type HeaderSearchField = {
  /** What the field shows. */
  text: Readonly<Ref<string>>
  type(text: string): void
  enter(): void
  focus(): void
  blur(): void
}

/**
 * The text of every `search` control of a header, kept by control id (spec 090-app-header-bar,
 * FR-012, FR-013): the field keeps what the user typed when the app hands a new description, and
 * keeps it while the control moves between the row and the overflow menu (a different element, the
 * same field). The rules are `lib/wm/headerSearch.ts`'s; this only makes them reactive and
 * gives them a clock.
 */
export function useHeaderSearches(emit: (event: WmHeaderEvent) => void) {
  const fields = new Map<
    string,
    { search: HeaderSearch; text: Ref<string>; field: HeaderSearchField }
  >()
  const timers = new Set<ReturnType<typeof setTimeout>>()

  function create(id: string, value: string) {
    const text = ref(value)
    const search = createHeaderSearch(value, {
      emitInput: (input) => emit({ id, kind: 'input', value: input }),
      emitSubmit: (input) => emit({ id, kind: 'submit', value: input }),
      schedule: (fn, ms) => {
        const timer = setTimeout(() => {
          timers.delete(timer)
          fn()
        }, ms)
        timers.add(timer)
        return () => {
          clearTimeout(timer)
          timers.delete(timer)
        }
      },
    })
    const sync = () => {
      text.value = search.text()
    }
    const field: HeaderSearchField = {
      text,
      type: (next) => {
        search.type(next)
        sync()
      },
      enter: () => search.enter(),
      focus: () => search.focus(),
      blur: () => {
        search.blur()
        sync()
      },
    }
    const entry = { search, text, field }
    fields.set(id, entry)
    return entry
  }

  /** A new description: every search control gets its `value`, the ones that are gone are dropped. */
  function describe(controls: readonly WmHeaderControl[]) {
    const seen = new Set<string>()
    for (const control of controls) {
      if (control.type !== 'search') continue
      seen.add(control.id)
      const entry = fields.get(control.id)
      if (entry) {
        entry.search.describe(control.value)
        entry.text.value = entry.search.text()
      } else {
        create(control.id, control.value)
      }
    }
    for (const id of [...fields.keys()]) if (!seen.has(id)) fields.delete(id)
  }

  /** The field of a search control (created from `value` when the control is new). */
  function fieldFor(id: string, value: string): HeaderSearchField {
    return (fields.get(id) ?? create(id, value)).field
  }

  onScopeDispose(() => {
    for (const timer of timers) clearTimeout(timer)
    timers.clear()
  })

  return { describe, fieldFor }
}
