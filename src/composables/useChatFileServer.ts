import { invoke } from '@tauri-apps/api/core'
import { computed, ref } from 'vue'

/**
 * Whether the vault has a Blossom-Server for chat files (spec 072 FR-038): a new vault has the
 * default one; without one, attaching files is not offered. Read again whenever the preferences change, so a server chosen in the
 * settings applies to the open conversation without reopening it.
 */
export function useChatFileServer() {
  const address = ref('')
  let latestRequest = 0

  async function refresh() {
    const request = ++latestRequest
    try {
      // The backend answers what a send uses: the default until the user chose, '' for none.
      const value = (await invoke<string | null>('chat_blossom_server')) ?? ''
      if (request === latestRequest) address.value = value
    } catch {
      if (request === latestRequest) address.value = ''
    }
  }
  void refresh()
  onVaultTablesChanged(['preferences'], refresh)

  return { hasServer: computed(() => address.value.trim() !== '') }
}
