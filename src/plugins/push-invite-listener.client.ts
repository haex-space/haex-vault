import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { createLogger } from '@/stores/logging'

const log = createLogger('PUSH-INVITE')

/**
 * Global listener for the `push-invite-received` Tauri event.
 *
 * Previously this listener was registered only when the Spaces settings
 * page was mounted, which meant invites that arrived while the user was
 * elsewhere silently landed in the DB without updating any reactive state.
 * Reloading the invites list requires a drizzle query, so the user saw no
 * badge, toast, or UI change until they manually navigated back to the
 * settings page — at which point a freshly created invite would
 * mysteriously "reveal" the older one.
 *
 * This plugin owns the listener app-wide so:
 * - The spaces store's `pendingInvites` stays in sync regardless of
 *   which window the user has focused.
 * - A toast surfaces the invite immediately.
 * - Unlisten runs on Nuxt app `close` so HMR doesn't leak handlers.
 */
export default defineNuxtPlugin({
  name: 'push-invite-listener',
  parallel: true,
  async setup(nuxtApp) {
    let unlisten: UnlistenFn | null = null

    try {
      unlisten = await listen('push-invite-received', async () => {
        log.info('Received push-invite-received event')
        try {
          const spacesStore = useSpacesStore()
          await spacesStore.loadSpacesFromDbAsync()
        } catch (error) {
          log.warn(`Failed to reload spaces after push invite: ${error}`)
        }

        try {
          const { add } = useToast()
          const isDe = (nuxtApp.$i18n?.locale?.value ?? 'de') === 'de'
          add({
            title: isDe ? 'Neue Einladung' : 'New invitation',
            description: isDe
              ? 'Du hast eine neue Space-Einladung erhalten.'
              : 'You received a new space invitation.',
            color: 'info',
          })
        } catch {
          // Toast composable not ready (e.g. very early startup) — skip
          // silently; the reload above is the important part.
        }
      })
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
