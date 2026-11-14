import { ref } from 'vue'
import { defineStore } from 'pinia'

/**
 * The pass a pinned shortcut asked for and that waits for a vault to be open (spec 092, FR-038). It
 * is only the opaque id; the workspace opens the pass in Wallet once the saved session is in place.
 * Reading it takes it out, so the next link can come in.
 */
export const usePassLinkInboxStore = defineStore('passLinkInbox', () => {
  const id = ref<string | null>(null)

  function park(next: string): void {
    id.value = next
  }

  /** The waiting id, and nothing waits afterwards. */
  function take(): string | null {
    const taken = id.value
    id.value = null
    return taken
  }

  return { id, park, take }
})
