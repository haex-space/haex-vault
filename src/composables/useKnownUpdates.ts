import { computed, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { CheckUpdatesResult } from '@bindings/CheckUpdatesResult'
import type { KnownUpdate } from '@bindings/KnownUpdate'
import type { KnownUpdatesView } from '@bindings/KnownUpdatesView'
import type { SourceError } from '@bindings/SourceError'

// What the host knows about updates (spec 069 US3, FR-017): the state is shared by every window
// of the app, so a check run in the catalog shows in the settings list at once. Nothing here asks
// a marketplace: `reloadKnownUpdatesAsync` reads what the host holds, and only
// `checkUpdatesAsync` makes the host ask, when the catalog app opens or on "Aktualisieren".
const view = ref<KnownUpdatesView>({ candidates: [], selected: [] })
export const updateSourceErrors = ref<SourceError[]>([])

let sequence = 0

/** Reads what the host knows, without any request to a marketplace. */
export async function reloadKnownUpdatesAsync(): Promise<void> {
  const mine = ++sequence
  try {
    const loaded = await invoke<KnownUpdatesView>('marketplace_known_updates')
    if (mine === sequence) view.value = loaded
  } catch {
    // The hint only shows what was found; without an answer there is nothing to show.
  }
}

/** Asks every marketplace that is on for a newer version of every installed extension, then
 * reads what the host now knows. The sources that failed are in the catalog's own error lines. */
export async function checkUpdatesAsync(): Promise<CheckUpdatesResult | null> {
  try {
    const result = await invoke<CheckUpdatesResult>('marketplace_check_updates')
    updateSourceErrors.value = result.sourceErrors
    await reloadKnownUpdatesAsync()
    return result
  } catch {
    updateSourceErrors.value = []
    return null
  }
}

/** The update the hint of an installed extension opens, by the extension's name. */
export function useKnownUpdates() {
  const extensions = useExtensionsStore()

  onMounted(() => void reloadKnownUpdatesAsync())
  // An install or removal changes which candidates still apply.
  watch(
    () => extensions.list.map((e) => `${e.name}@${e.version ?? ''}`).join('|'),
    () => void reloadKnownUpdatesAsync(),
  )
  onVaultTablesChanged(['haex_marketplaces'], reloadKnownUpdatesAsync)

  const byName = computed(
    () =>
      new Map<string, KnownUpdate>(
        view.value.selected.map((update) => [update.installedName, update]),
      ),
  )
  return { byName }
}
