import { usePreferences } from '~/composables/usePreferences'
import type { useWindowManagerStore } from '~/stores/windowManager'

type WmStore = ReturnType<typeof useWindowManagerStore>

/** The vault-scoped preference `VoiceInputControl.vue` reads on mount (spec 008). */
export const VOICE_AUTO_SEND_PREF_KEY = 'voice.auto_send'

/**
 * Global handlers of the chat's model, reasoning and voice actions (spec
 * 020-tab-navigation, T050, `CHAT_MODEL_ACTIONS` in
 * `lib/actions/chatActions.ts`). They act on the global models store and
 * preferences, so they work without the chat being open.
 */
export function registerChatActionHandlers(wm: WmStore): void {
  // The models store calls `useI18n()` in its setup, which only works inside a component setup:
  // resolve it per call (the workspace page has created it by then), never at registration.
  const models = () => useModelsStore()
  const { setPrefAsync } = usePreferences()
  const done = { done: true }
  const on = wm.registerGlobalActionHandler

  function beginModelAction() {
    const store = models()
    store.lastError = null
    store.integrityActionError = null
    return store
  }

  function ensureModelActionSucceeded(store: ReturnType<typeof models>) {
    const error = store.lastError ?? store.integrityActionError
    if (error) throw new Error(error)
  }

  on('chat.model.select', async ({ input, caller }) => {
    const store = beginModelAction()
    await store.loadModel(String(input.modelId), {
      pick: true,
      threadId:
        caller.kind === 'user' && typeof input.threadId === 'string'
          ? input.threadId
          : null,
    })
    ensureModelActionSucceeded(store)
    return done
  })
  on('chat.reasoning.set', async ({ input }) => {
    const store = beginModelAction()
    await store.updateEffortLevel(
      typeof input.level === 'string' ? input.level : null,
    )
    ensureModelActionSucceeded(store)
    return done
  })
  on('chat.model.retryLoad', async () => {
    const store = beginModelAction()
    await store.retryModelLoad()
    ensureModelActionSucceeded(store)
    return done
  })
  on('chat.model.downloadRecommended', async ({ input }) => {
    const store = beginModelAction()
    const entry = store.catalogEntries.find((e) => e.id === input.entryId)
    if (!entry) throw new Error(`no catalog entry ${String(input.entryId)}`)
    await store.downloadCatalogEntry(entry)
    ensureModelActionSucceeded(store)
    return done
  })
  on('chat.modelIntegrity.decide', async ({ input }) => {
    const store = beginModelAction()
    if (input.decision === 'loadUntrusted')
      await store.onIntegrityLoadUntrusted()
    else if (input.decision === 'repairSource')
      await store.onIntegrityRepairSource()
    else await store.onIntegrityChooseOther()
    ensureModelActionSucceeded(store)
    return done
  })
  on('chat.voice.setAutoSend', async ({ input }) => {
    await setPrefAsync(
      { kind: 'vault' },
      VOICE_AUTO_SEND_PREF_KEY,
      String(input.enabled === true),
    )
    return done
  })
}
