import { useAssistantStore } from '~/stores/assistant'
import type { useWindowManagerStore } from '~/stores/windowManager'

type WmStore = ReturnType<typeof useWindowManagerStore>

/**
 * Global handler of the voice assistant's toggle (spec 066 T049,
 * `lib/actions/assistantActions.ts`). `language` gives the window's language at the time of the
 * call, the language of the conversation.
 */
export function registerAssistantActionHandlers(
  wm: WmStore,
  language: () => string,
): void {
  // Resolved per call: the store needs the active Pinia, which exists by the time it is clicked.
  const assistant = () => useAssistantStore()

  wm.registerGlobalActionHandler('assistant.toggle', async () => {
    const store = assistant()
    await store.start()
    const enabled = !store.state.enabled
    // Until the wake word, the assistant is reachable only with the preview setting (T045);
    // switching off always works.
    if (enabled && !store.preview) return { enabled: false }
    await store.setEnabledAsync(enabled, language())
    return { enabled: store.state.enabled }
  })
}
