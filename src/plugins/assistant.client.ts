import { watch } from 'vue'
import { toast } from 'vue-sonner'
import { useAssistantStore } from '~/stores/assistant'

/**
 * Shows what keeps the voice assistant from working (spec 066, contracts/tauri-commands.md
 * `assistant-error`): a toggle that could not switch on (a missing model, no microphone) or a
 * round that could not start. The toggle lives in the dock and the launcher, which have no room
 * for an error, so it comes as a toast.
 */
export default defineNuxtPlugin((nuxtApp) => {
  const assistant = useAssistantStore()
  const t = (key: string, params: Record<string, unknown>) =>
    nuxtApp.$i18n.t(key, params)
  watch(
    () => assistant.lastError,
    (failure) => {
      if (failure) {
        toast.error(t('assistant.failed', { reason: reasonOf(failure.error) }))
      }
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
