import { computed, ref } from 'vue'

const KEY = 'chat.blossom.server'

/**
 * Whether the vault has a Blossom-Server for chat files (spec 072 FR-038): without one, attaching
 * files is not offered. Read again whenever the preferences change, so a server chosen in the
 * settings applies to the open conversation without reopening it.
 */
export function useChatFileServer() {
  const { getPrefAsync } = usePreferences()
  const address = ref('')
  let latestRequest = 0

  async function refresh() {
    const request = ++latestRequest
    try {
      const value = (await getPrefAsync({ kind: 'vault' }, KEY)) ?? ''
      if (request === latestRequest) address.value = value
    } catch {
      if (request === latestRequest) address.value = ''
    }
  }
  void refresh()
  onVaultTablesChanged(['preferences'], refresh)

  return { hasServer: computed(() => address.value.trim() !== '') }
}
