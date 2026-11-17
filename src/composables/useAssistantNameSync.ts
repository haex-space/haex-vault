import { watch } from 'vue'
import {
  clearAssistantName,
  loadAssistantNameAsync,
} from '~/composables/useAssistantName'
import { onVaultTablesChanged } from '~/composables/useVaultData'
import { useInstancesStore } from '~/stores/instances'

/**
 * Keeps the name of the assistant (one per vault, design 2026-11-16) in step with the open vault:
 * read when a vault opens and after every change of the preferences, also one made on another
 * device. Without a vault the default name stays.
 *
 * Called from the setup of `app.vue`, not from a plugin: the instances store reads `useI18n()` in
 * its setup, which only works while a component is being set up. A plugin runs before the first
 * component and built the store too early, so the app did not start ("Must be called at the top
 * of a `setup` function"). The watch and the subscription end with the app's scope.
 */
export function useAssistantNameSync() {
  const instances = useInstancesStore()
  const reload = () =>
    loadAssistantNameAsync().catch((error: unknown) =>
      console.warn('[assistant-name] could not be read', error),
    )
  watch(
    () => instances.activeInstance,
    (active) => {
      if (active) void reload()
      else clearAssistantName()
    },
    { immediate: true },
  )
  onVaultTablesChanged(['preferences'], () => {
    if (instances.activeInstance) return reload()
  })
}
