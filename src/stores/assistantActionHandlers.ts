import { useAssistantStore } from '~/stores/assistant'
import type { useWindowManagerStore } from '~/stores/windowManager'

type WmStore = ReturnType<typeof useWindowManagerStore>

/**
 * Global handlers of the voice assistant's actions (spec 066 T049,
 * `lib/actions/assistantActions.ts`). `language` gives the window's language at the time of the
 * call, the language of the conversation.
 */
export function registerAssistantActionHandlers(
  wm: WmStore,
  language: () => string,
): void {
  const done = { done: true }
  const notDone = { done: false }
  const on = wm.registerGlobalActionHandler
  // Resolved per call: the store needs the active Pinia, which exists by the time a key is pressed.
  const assistant = () => useAssistantStore()

  on('assistant.talk', async () => {
    const store = assistant()
    await store.start()
    // Until the wake word, conversations are reachable only with the preview setting (T045).
    if (!store.preview) return notDone
    await store.talkAsync(language())
    return done
  })
  on('assistant.mute.set', async ({ input }) => {
    await assistant().setMutedAsync(input.muted === true)
    return done
  })
  on('assistant.conversation.end', async () => {
    await assistant().endConversationAsync()
    return done
  })
  on('assistant.conversation.show', async () => {
    const store = assistant()
    const threadId = store.state.threadId ?? store.entriesThread
    if (!threadId) return notDone
    const outcome = await wm.runAction('chat.conversation.open', { threadId })
    if (!outcome.ok) throw new Error(outcome.message)
    return done
  })
}
