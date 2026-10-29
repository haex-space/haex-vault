import { ref } from 'vue'
import { defineStore } from 'pinia'

/**
 * The mail link that came in from outside and waits for the person to choose what opens it
 * (spec 063 FR-017). At most one waits: a new link replaces it. It lives in memory only, is kept
 * while the vault is locked and across the move from the start page to the workspace (the same
 * process), and is never saved. Reading it takes it out.
 */
export const useMailInboxStore = defineStore('mailInbox', () => {
  const link = ref<string | null>(null)

  function park(next: string): void {
    link.value = next
  }

  /** The waiting link, and nothing waits afterwards. */
  function take(): string | null {
    const taken = link.value
    link.value = null
    return taken
  }

  return { link, park, take }
})
