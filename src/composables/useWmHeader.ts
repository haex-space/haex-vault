import { onScopeDispose, watchEffect } from 'vue'
import { useWmTab } from '~/composables/useWmTab'
import { headerProblems } from '~/lib/wm/headerChecks'
import type { WmHeaderDescription } from '~/types/bindings/WmHeaderDescription'
import type { WmHeaderEvent } from '~/types/bindings/WmHeaderEvent'

/**
 * A system app's way to put controls into its window's header (spec 090-app-header-bar, FR-007,
 * FR-022; extensions use the SDK's `header.set` instead). `description` is read reactively: every
 * change hands holzi the new description whole, so the app keeps its controls up to date by
 * deriving them from its state. `onEvent` receives what the user does to a control (FR-012). Both
 * end with the app's scope: the controls leave the header when the app goes away, so a tab never
 * shows another app's controls. Returning `null` gives the header back to the title alone.
 *
 * A system app is trusted code, so nothing is refused; a development build warns about what is
 * obviously wrong (contracts/header-description.md §System apps).
 */
export function useWmHeader(
  description: () => WmHeaderDescription | null,
  onEvent: (event: WmHeaderEvent) => void,
): void {
  const tab = useWmTab()

  const unregister = tab.registerHeaderHandler(onEvent)
  const stop = watchEffect(() => {
    const next = description()
    if (import.meta.dev && next) {
      for (const problem of headerProblems(next))
        console.warn(`[header] ${tab.appId}: ${problem}`)
    }
    tab.setHeader(next)
  })

  onScopeDispose(() => {
    stop()
    unregister()
    tab.setHeader(null)
  })
}
