import type { Sources } from '@bindings/Sources'
import type { SourceRef } from '@bindings/SourceRef'
import { offerPin } from '~/lib/files/pins'

/**
 * What a panel's context menu does with pinned folders (spec 071): which pin command a folder of
 * the open source gets, and the change itself. The pins are the device preference of the sidebar.
 */
export function useFilesPinCommands(
  source: Ref<SourceRef>,
  sources: Ref<Sources | null>,
) {
  const prefs = useFilesPrefs()

  const offer = (path: string) =>
    offerPin(
      prefs.pins.value,
      { source: source.value, path },
      sources.value?.known.map((place) => place.path) ?? [],
    )

  function set(path: string, on: boolean) {
    const pin = { source: source.value, path }
    if (on) prefs.pin([pin])
    else prefs.unpin(pin)
  }

  return { offer, set }
}
