import { ref } from 'vue'
import { defineStore } from 'pinia'
import type { SharedCapture } from '~/lib/wallet/sharedRoute'
import { releaseSharedFile } from '~/lib/wallet/sharedFiles'

/**
 * The pictures and PDFs another app shared to holzi, waiting for the capture dialog of Wallet
 * (spec 092, FR-037). The dialog takes one at a time; what is dropped unread has its copy removed.
 */
export const useWalletCaptureStore = defineStore('walletCapture', () => {
  const files = ref<SharedCapture[]>([])

  function enqueue(next: SharedCapture[]): void {
    files.value = [...files.value, ...next]
  }

  /** The next file, and it no longer waits. */
  function take(): SharedCapture | null {
    const [first, ...rest] = files.value
    if (first === undefined) return null
    files.value = rest
    return first
  }

  function clear(): void {
    const dropped = files.value
    files.value = []
    for (const { file } of dropped) void releaseSharedFile(file)
  }

  return { files, enqueue, take, clear }
})
