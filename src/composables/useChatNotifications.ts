import { onScopeDispose } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { toast } from 'vue-sonner'
import { contactChatPath } from '~/composables/useChatNavigation'
import { raiseWindowAsync } from '~/composables/useExtensionHost'
import { useWindowManagerStore } from '~/stores/windowManager'

/**
 * Notifications of new contact messages on the workspace page (spec 072 FR-023, research R17): a
 * tap on the system's notification opens the chat at the conversation; where the system shows
 * none (Android: permission refused), holzi shows it as a message with "Öffnen".
 */
export function useChatNotifications() {
  const wm = useWindowManagerStore()
  const { t } = useI18n()
  let disposed = false
  const unlisteners: UnlistenFn[] = []
  onScopeDispose(() => {
    disposed = true
    for (const unlisten of unlisteners) unlisten()
  })
  const keep = (listening: Promise<UnlistenFn>, what: string) => {
    listening
      .then((unlisten) => {
        if (disposed) unlisten()
        else unlisteners.push(unlisten)
      })
      .catch((error: unknown) => {
        console.error(`[chat] listening to ${what} failed`, error)
      })
  }
  const open = (threadId: string) => {
    wm.openApp('system.chat', contactChatPath(threadId))
    void raiseWindowAsync()
  }
  keep(
    listen<{ threadId: string }>('chat-notification-open', (event) =>
      open(event.payload.threadId),
    ),
    'notification taps',
  )
  keep(
    listen<{ threadId: string; title: string; body: string | null }>(
      'chat-notification-in-app',
      (event) => {
        const { threadId, title, body } = event.payload
        // A newer message of the conversation replaces its earlier one, like on the system.
        toast(title, {
          id: `chat:${threadId}`,
          description: body ?? undefined,
          action: {
            label: t('chat.notify.open'),
            onClick: () => open(threadId),
          },
        })
      },
    ),
    'notifications shown in holzi',
  )
}
