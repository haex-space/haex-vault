import type { SourceRef } from '@bindings/SourceRef'

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
  const folderOwned = ref(false)
  const folderEncrypted = ref(false)
  /** How far the open folder can be read when it is encrypted or in an encrypted folder, else `null`. */
  const folderEncryptedState = ref<string | null>(null)
  const folderForeign = ref(0)
  watch(
    [source, path],
    async ([current, folderPath]) => {
      folderOwned.value = false
      folderEncrypted.value = false
      folderEncryptedState.value = null
      folderForeign.value = 0
      if (folderPath === null) return
      const entry = await statAsync(current, folderPath).catch(() => null)
      if (path.value === folderPath) {
        folderOwned.value = entry?.holziOwned ?? false
        folderEncrypted.value = !!entry?.encrypted
        folderEncryptedState.value = entry?.encrypted?.state ?? null
        folderForeign.value = entry?.encrypted?.foreign ?? 0
      }
    },
    { immediate: true },
  )
  return { folderOwned, folderEncrypted, folderEncryptedState, folderForeign }
}
