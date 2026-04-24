import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { createLogger } from '@/stores/logging'

const log = createLogger('PUSH-INVITE')

/**
 * Global listener for the `push-invite-received` Tauri event.
 *
 * Surfaces a toast app-wide so the user notices an invite even when the
 * Spaces settings page is not mounted. The invite list itself is
 * refreshed by `useSpaceInvites` (scoped to the settings page):
 * - While the settings page IS mounted, the composable's own
 *   `listenForPushInvitesAsync` listener reloads the list.
 * - While the settings page is NOT mounted, there is no reactive invite
 *   view to update — the next `onMounted` on the settings page calls
 *   `loadInvitesAsync` and picks up the new row.
 *
 * The handler intentionally does NOT call `loadSpacesFromDbAsync`: the
 * push-invite backend writes to `haex_pending_invites` only, so reloading
 * `haex_spaces` would be a no-op.
 *
 * Unlisten runs on Nuxt app HMR dispose so reloads don't leak handlers.
 */
export default defineNuxtPlugin({
  name: 'push-invite-listener',
  parallel: true,
  async setup(nuxtApp) {
    let unlisten: UnlistenFn | null = null

    try {
      unlisten = await listen('push-invite-received', async () => {
        log.info('Received push-invite-received event — surfacing toast')

        let toastOk = false
        try {
          const { add } = useToast()
          const i18n = nuxtApp.$i18n as { locale?: { value?: string } } | undefined
          const isDe = (i18n?.locale?.value ?? 'de') === 'de'
          add({
            title: isDe ? 'Neue Einladung' : 'New invitation',
            description: isDe
              ? 'Du hast eine neue Space-Einladung erhalten.'
              : 'You received a new space invitation.',
            color: 'info',
          })
          toastOk = true
        } catch (error) {
          // Previously swallowed silently — which is exactly how a missing
          // toast turns into "the user has no idea an invite arrived".
          // Logging here makes the failure diagnosable from DB logs alone.
          log.warn(`Failed to surface invite toast: ${error}`)
        }

        log.info(`push-invite handler completed — toast=${toastOk}`)
      })
      log.info('Registered global push-invite-received listener')
    } catch (error) {
      log.warn(`Failed to register push-invite listener: ${error}`)
    }

    if (import.meta.hot) {
      import.meta.hot.dispose(() => {
        unlisten?.()
      })
    }
  },
})
