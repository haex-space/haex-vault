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
        log.info('Received push-invite-received event — reloading spaces + adding toast')

        let reloadOk = false
        try {
          const spacesStore = useSpacesStore()
          await spacesStore.loadSpacesFromDbAsync()
          reloadOk = true
        } catch (error) {
          log.warn(`Failed to reload spaces after push invite: ${error}`)
        }

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
          // Logging here makes the listener-vs-toast split diagnosable from
          // production DB logs alone.
          log.warn(`Failed to surface invite toast: ${error}`)
        }

        log.info(`push-invite handler completed — spacesReload=${reloadOk} toast=${toastOk}`)
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
