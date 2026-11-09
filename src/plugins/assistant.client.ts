import { watch } from 'vue'
import { toast } from 'vue-sonner'
import { useAssistantStore } from '~/stores/assistant'
import { hintActions } from '~/lib/assistant/speechModel'

/**
 * Shows what keeps the voice assistant from working (spec 066, contracts/tauri-commands.md
 * `assistant-error`): a toggle that could not switch on (a missing model, no microphone) or a
 * round that could not start. The toggle lives in the dock and the launcher, which have no room
 * for an error, so it comes as a toast. So do the speech model notes of spec 081: the offer of a
 * faster model when the chosen one did not keep up (FR-011), and Pocket speaking instead of the
 * chosen model.
 */
export default defineNuxtPlugin((nuxtApp) => {
  const assistant = useAssistantStore()
  const t = (key: string, params: Record<string, unknown> = {}) =>
    nuxtApp.$i18n.t(key, params)
  watch(
    () => assistant.lastError,
    (failure) => {
      if (failure) {
        toast.error(t('assistant.failed', { reason: reasonOf(failure.error) }))
      }
    },
  )
  watch(
    () => assistant.speechModelHint,
    (hint) => {
      if (!hint) return
      const actions = hintActions(hint)
      const write = (pref: { key: string; value: string }) =>
        assistant
          .setDevicePrefAsync(pref.key, pref.value)
          .catch((e: unknown) =>
            toast.error(
              t('settings.voice.saveFailed', { reason: reasonOf(e) }),
            ),
          )
      // Every way out writes "dismissed", so the hint shows once per device (FR-011).
      let answered = false
      const dismiss = () => {
        if (answered) return
        answered = true
        void write(actions.dismiss)
      }
      toast(t('assistant.speechModelHint.title'), {
        description: t('assistant.speechModelHint.description'),
        duration: Number.POSITIVE_INFINITY,
        action: {
          label: t('assistant.speechModelHint.switch'),
          onClick: () => {
            void write(actions.switchTo)
            dismiss()
          },
        },
        cancel: {
          label: t('assistant.speechModelHint.dismiss'),
          onClick: dismiss,
        },
        onDismiss: dismiss,
      })
    },
  )
  watch(
    () => assistant.speechModelFallback,
    (fallback) => {
      if (fallback)
        toast.warning(t(`assistant.speechModelFallback.${fallback.reason}`))
    },
  )
})

/** The reason of a backend error (`{kind, reason}`), a message, or the kind. */
function reasonOf(error: unknown): string {
  if (typeof error === 'string') return error
  if (error && typeof error === 'object') {
    const record = error as Record<string, unknown>
    for (const key of ['reason', 'message', 'kind']) {
      if (typeof record[key] === 'string') return record[key]
    }
  }
  return String(error)
}
