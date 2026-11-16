import { watch } from 'vue'
import { loadAssistantNameAsync } from '~/composables/useAssistantName'
import { onVaultTablesChanged } from '~/composables/useVaultData'
import { useInstancesStore } from '~/stores/instances'

/**
 * Keeps the name of the assistant (one per vault, design 2026-11-16) in step with the open vault:
 * read when a vault opens and after every change of the preferences, also one made on another
 * device. Without a vault the default name stays.
 */
export default defineNuxtPlugin(() => {
  const instances = useInstancesStore()
  const reload = () =>
    loadAssistantNameAsync().catch((error: unknown) =>
      console.warn('[assistant-name] could not be read', error),
    )
  watch(
    () => instances.activeInstance,
    (active) => {
      if (active) void reload()
    },
    { immediate: true },
  )
  onVaultTablesChanged(['preferences'], () => {
    if (instances.activeInstance) return reload()
  })
})
