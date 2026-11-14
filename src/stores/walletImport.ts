import { ref } from 'vue'
import { defineStore } from 'pinia'
import type { PickedFile } from '@bindings/PickedFile'
import { releaseSharedFile } from '~/lib/wallet/sharedFiles'

/**
 * The pass files waiting to be imported (spec 092, FR-031): the toolbar of Wallet puts the files
 * the person chose here, the import dialog takes them one by one, each its own unit. The Android
 * share target (US8) puts a shared pkpass here too. A file is only a path or an address; the
 * backend reads it.
 */
export const useWalletImportStore = defineStore('walletImport', () => {
  const files = ref<PickedFile[]>([])

  function enqueue(next: PickedFile[]): void {
    files.value = [...files.value, ...next]
  }

  /** The next file, and it no longer waits. */
  function take(): PickedFile | null {
    const [first, ...rest] = files.value
    if (first === undefined) return null
    files.value = rest
    return first
  }

  /** Drops what waits; the copy of a shared file that was never read is removed (FR-035). */
  function clear(): void {
    const dropped = files.value
    files.value = []
    for (const file of dropped) void releaseSharedFile(file)
  }

  return { files, enqueue, take, clear }
})
