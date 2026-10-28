import type { FilesPanelContext } from '~/composables/useFilesSplit'
import type { FilesView } from '~/composables/useFilesPrefs'
import { formatSort, parseSort, type Sort } from '~/lib/files/state'

/**
 * How one panel shows its folder (spec 062: view and sort are per panel). The device's preference
 * (`useFilesPrefs`) is the default; a panel that was changed while two show keeps its own in the
 * tab location (`vw`, `so`), so session restore brings it back. With one panel the change is the
 * device's, as before, and the panel's own override ends.
 */
export function useFilesPanelView(context: FilesPanelContext) {
  const prefs = useFilesPrefs()
  const place = context.place

  const view = computed((): FilesView =>
    place.value.vw === 'grid' || place.value.vw === 'list'
      ? place.value.vw
      : prefs.view.value,
  )
  const sort = computed((): Sort =>
    place.value.so ? parseSort(place.value.so) : prefs.sort.value,
  )

  function setView(next: FilesView) {
    if (context.split.value) {
      context.setQuery({ vw: next })
      return
    }
    prefs.setView(next)
    context.setQuery({ vw: null })
  }

  function setSort(next: Sort) {
    if (context.split.value) {
      context.setQuery({ so: formatSort(next) })
      return
    }
    prefs.setSort(next)
    context.setQuery({ so: null })
  }

  return { view, sort, setView, setSort }
}
