import { toast } from 'vue-sonner'
import type { Translate } from '~/composables/useModelInventory'
import { setupPath } from '~/lib/assistant/indicator'
import { useAssistantStore } from '~/stores/assistant'
import type { useWindowManagerStore } from '~/stores/windowManager'

type WmStore = ReturnType<typeof useWindowManagerStore>

/**
 * Global handler of the voice assistant's toggle (spec 066 T049,
 * `lib/actions/assistantActions.ts`). `language` gives the window's language at the time of the
 * call, the language of the conversation.
 *
 * Switching on needs everything in place (FR-005): when something is missing, a toast names it and
 * the settings open on the page that sets it up (operator 2026-11-10); the assistant stays off.
 * The first time, the notice of FR-037 must be confirmed. Switching off always works.
 */
export function registerAssistantActionHandlers(
  wm: WmStore,
  language: () => string,
  t: Translate,
): void {
  // Resolved per call: the store needs the active Pinia, which exists by the time it is clicked.
  const assistant = () => useAssistantStore()

  wm.registerGlobalActionHandler('assistant.toggle', async () => {
    const store = assistant()
    await store.start()
    const enabled = !store.state.enabled
    if (enabled) {
      const missing = await store.missingAsync(language())
      if (missing.length > 0) {
        toast.info(
          t('assistant.missing.title', {
            items: missing
              .map((item) => t(`assistant.missing.${item}`))
              .join(', '),
          }),
        )
        const path = setupPath(missing)
        if (path !== null) wm.openApp('system.settings', path)
        return { enabled: false }
      }
      if (!(await store.confirmNoticeAsync())) return { enabled: false }
    }
    await store.setEnabledAsync(enabled, language())
    return { enabled: store.state.enabled }
  })
}
