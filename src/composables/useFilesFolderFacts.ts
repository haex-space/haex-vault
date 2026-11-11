import type { SourceRef } from '@bindings/SourceRef'
import type { Entry } from '@bindings/Entry'

/**
 * What the open folder is, from its own entry (spec 044 FR-037, spec 048): one of holzi's own places
 * (nothing changes in it), in or below an encrypted folder, and how many foreign files an encrypted
 * folder holds (FR-048, 0 below its top), and its readable state (spec 073). They reset while the next folder is being read.
 */
export function useFilesFolderFacts(
  source: Ref<SourceRef>,
  path: Ref<string | null>,
) {
  const { statAsync } = useFiles()
  const folderEntry = ref<Entry | null>(null)
  const folderOwned = computed(() => folderEntry.value?.holziOwned ?? false)
  const folderEncrypted = computed(() => !!folderEntry.value?.encrypted)
  const folderForeign = computed(
    () => folderEntry.value?.encrypted?.foreign ?? 0,
  )
  watch(
    [source, path],
    async ([current, folderPath], _previous, onCleanup) => {
      folderEntry.value = null
      let active = true
      onCleanup(() => {
        active = false
      })
      if (folderPath === null) return
      const entry = await statAsync(current, folderPath).catch(() => null)
      if (active && source.value === current && path.value === folderPath)
        folderEntry.value = entry
    },
    { immediate: true, flush: 'sync' },
  )
  return { folderEntry, folderOwned, folderEncrypted, folderForeign }
}
