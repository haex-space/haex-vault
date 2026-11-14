import { ref } from 'vue'
import { defineStore } from 'pinia'

/**
 * Whether the chat's sidebar is hidden in a wide window (spec 076 FR-008). Kept here so it survives
 * the chat tab being left and opened again in the same window; not stored and not synced.
 */
export const useChatSidebarStore = defineStore('chatSidebar', () => {
  const hidden = ref(false)
  return { hidden }
})
