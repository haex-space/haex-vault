import type { SourceRef } from '@bindings/SourceRef'

/**
 * What the open folder is, from its own entry (spec 044 FR-037, spec 048): one of holzi's own places
 * (nothing changes in it), in or below an encrypted folder, and how many foreign files an encrypted
 * folder holds (FR-048, 0 below its top). They reset while the next folder is being read.
 */
export function useFilesFolderFacts(
  source: Ref<SourceRef>,
  path: Ref<string | null>,
) {
  const { statAsync } = useFiles()
  const folderOwned = ref(false)
  const folderEncrypted = ref(false)
  const folderForeign = ref(0)
  watch(
    [source, path],
    async ([current, folderPath]) => {
      folderOwned.value = false
      folderEncrypted.value = false
      folderForeign.value = 0
      if (folderPath === null) return
      const entry = await statAsync(current, folderPath).catch(() => null)
      if (path.value === folderPath) {
        folderOwned.value = entry?.holziOwned ?? false
        folderEncrypted.value = !!entry?.encrypted
        folderForeign.value = entry?.encrypted?.foreign ?? 0
      }
    },
    { immediate: true },
  )
  return { folderOwned, folderEncrypted, folderForeign }
}
