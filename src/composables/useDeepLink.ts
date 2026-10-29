import { getCurrent, onOpenUrl } from '@tauri-apps/plugin-deep-link'
import { listen } from '@tauri-apps/api/event'
import { parkOrOpen, type ReceivedLink } from '~/lib/contacts/deepLink'
import { acceptMailLink } from '~/lib/mail/link'

/** Sent by Rust when another holzi process passed on a link (`deep_link::RECEIVED_EVENT`). */
const RECEIVED_EVENT = 'deep-link-received'

let started = false
let last: ReceivedLink | null = null

/**
 * Links opened outside holzi: of a share (spec 060 FR-018a, research R9) and mail links (spec 063
 * FR-017), each parked in its own inbox. Three ways bring one: the
 * link a start was made with (`getCurrent`), a link while holzi runs or a cold start on Android,
 * which arrives only after `setup` through `onOpenUrl`, and another holzi process passing one on
 * (`deep-link-received`). The link is parked in its inbox; the workspace page opens the preview
 * or the choice once a vault is open. Nothing is saved from here.
 */
export function useDeepLink() {
  const inbox = useShareInboxStore()
  const mailInbox = useMailInboxStore()

  function handle(url: unknown): void {
    const now = Date.now()
    const mail = acceptMailLink(url, last, now)
    if (mail !== null) {
      last = { text: mail, at: now }
      mailInbox.park(mail)
      return
    }
    const text = parkOrOpen(url, inbox.text, last, now)
    if (text === null) return
    last = { text, at: now }
    inbox.park(text)
  }

  async function init(): Promise<void> {
    if (started) return
    started = true
    try {
      for (const url of (await getCurrent()) ?? []) handle(url)
      await onOpenUrl((urls) => urls.forEach(handle))
      await listen<string>(RECEIVED_EVENT, (event) => handle(event.payload))
    } catch (error) {
      // Outside the app (a browser during development) there is nothing to listen to.
      console.error('[deep-link] listening for links failed', error)
    }
  }

  return { init }
}
