import { ref } from 'vue'
import { defineStore } from 'pinia'

/**
 * The text of a share that came in as a link and waits for the person to look at it (spec 060
 * FR-018a). It lives here, not in the address: 16 KiB in a query parameter would break. It is kept
 * while no vault is open and survives the move from the start page to the workspace of the vault
 * (the same process). Reading it takes it out, so the next link can come in.
 */
export const useShareInboxStore = defineStore('shareInbox', () => {
  const text = ref<string | null>(null)

  function park(next: string): void {
    text.value = next
  }

  /** The waiting text, and nothing waits afterwards. */
  function take(): string | null {
    const taken = text.value
    text.value = null
    return taken
  }

  return { text, park, take }
})
