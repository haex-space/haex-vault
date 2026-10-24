import { onScopeDispose, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { toast } from 'vue-sonner'
import type { DevModeState } from '@bindings/DevModeState'
import type { ExtensionPeerInvitation } from '@bindings/ExtensionPeerInvitation'
import type { ExtensionPeerInvitationGone } from '@bindings/ExtensionPeerInvitationGone'
import { useColorScheme } from '~/composables/useColorScheme'
import { readyFrames } from '~/lib/extensions/agentFrames'
import { extensionAppId } from '~/lib/extensions/apps'
import { useExtensionPeerInvitationsStore } from '~/stores/extensionPeerInvitations'
import { useExtensionPermissionsStore } from '~/stores/extensionPermissions'
import { useExtensionsStore } from '~/stores/extensions'
import { useWindowManagerStore } from '~/stores/windowManager'

/**
 * The extension host on the workspace page (spec 017): the extension list for the app list of the
 * window manager, and the color scheme and language holzi applies, which extensions read through
 * `extension_context_get` (contracts/tauri-commands.md `extension_host_context_set`).
 */
/** Set before the one reload for developer mode, so a document that still lacks the development
 * origins (in `tauri dev` holzi's hook does not run) does not reload again and again. */
const DEV_RELOAD_KEY = 'holzi.extensions.devReload'

/** Developer mode is on, but this document was loaded before holzi knew it (before unlocking): its
 * policy cannot frame development servers yet, so the window reloads once (spec 017, US12,
 * research R16). Returns whether it reloads. */
async function reloadForDevModeAsync(): Promise<boolean> {
  let state: DevModeState
  try {
    state = await invoke<DevModeState>('extension_dev_mode_get')
  } catch {
    return false
  }
  try {
    if (!state.enabled || state.framesAllowed) {
      sessionStorage.removeItem(DEV_RELOAD_KEY)
      return false
    }
    if (sessionStorage.getItem(DEV_RELOAD_KEY) !== null) return false
    sessionStorage.setItem(DEV_RELOAD_KEY, '1')
  } catch {
    // Without session storage no loop guard: no reload.
    return false
  }
  window.location.reload()
  return true
}

/** Brings this native window to the front. Mobile has no such window calls; their failure leaves
 * the window as it is. */
/** How long a call of the agent waits for an extension's frame (spec 018, research R9). */
const AGENT_FRAME_TIMEOUT_MS = 10_000

export async function raiseWindowAsync(): Promise<void> {
  const window = getCurrentWindow()
  for (const step of [
    () => window.unminimize(),
    () => window.show(),
    () => window.setFocus(),
  ]) {
    try {
      await step()
    } catch {
      // Not offered on this platform.
    }
  }
}

export function useExtensionHost() {
  const extensions = useExtensionsStore()
  const permissions = useExtensionPermissionsStore()
  const invitations = useExtensionPeerInvitationsStore()
  const { scheme } = useColorScheme()
  const { locale, t } = useI18n()

  watch(
    [scheme, locale],
    ([theme, language]) => {
      void invoke('extension_host_context_set', {
        theme,
        locale: language,
      }).catch((error: unknown) => {
        console.error('[extensions] reporting the context failed', error)
      })
    },
    { immediate: true },
  )

  // A click on a notification of an extension brings its tab forward (spec 017 US8, FR-052), and
  // the native window that now shows the tab comes to the front, no other. The extension itself
  // hears the click through its frame. Registered once per scope; a listener that arrives after the
  // scope ended is dropped at once, and a failure does not stop the start.
  const wm = useWindowManagerStore()
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
        console.error(`[extensions] listening to ${what} failed`, error)
      })
  }
  const showExtension = (extensionId: string) => {
    if (wm.showExtension(extensionId)) void raiseWindowAsync()
  }
  keep(
    listen<{ extensionId: string }>('extension-notification-click', (event) =>
      showExtension(event.payload.extensionId),
    ),
    'notification clicks',
  )
  // An extension leads the person to the models it may use (spec 055 US9); Rust names the calling
  // extension, never one the extension chose.
  keep(
    listen<{ extensionId: string }>(
      'extension-open-model-settings',
      (event) => {
        const id = encodeURIComponent(event.payload.extensionId)
        wm.openApp('system.settings', `/extensions/${id}?section=models`)
        void raiseWindowAsync()
      },
    ),
    'requests for the model settings',
  )
  // Where the system shows no notification of an extension (Android: the person refused the
  // permission, spec 043 FR-023), holzi shows it as a message; "open" brings the extension's tab.
  keep(
    listen<{
      extensionId: string
      title: string
      body: string | null
      tag: string | null
    }>('extension-notification-in-app', (event) => {
      const { extensionId, title, body, tag } = event.payload
      toast(title, {
        // Like on the system, a tagged message replaces the extension's earlier one.
        id: tag === null ? undefined : `${extensionId}:${tag}`,
        description: body ?? undefined,
        action: {
          label: t('extensions.notification.open'),
          onClick: () => showExtension(extensionId),
        },
      })
    }),
    'notifications shown in holzi',
  )

  // An invitation of a contact to an extension (spec 057 US1): holzi's own prompt asks the user,
  // and the window comes forward when the user clicked the notification that told of it. The
  // prompt goes when the invitation does (withdrawn, expired, taken by another device).
  keep(
    listen<ExtensionPeerInvitation>('extension-peer-invitation', (event) => {
      invitations.add(event.payload)
      if (event.payload.raise) void raiseWindowAsync()
    }),
    'invitations of contacts',
  )
  keep(
    listen<ExtensionPeerInvitationGone>(
      'extension-peer-invitation-gone',
      (event) => invitations.gone(event.payload.inviteId),
    ),
    'withdrawn invitations',
  )

  // A call of the agent needs a frame of the extension (spec 018, research R9): an open tab in
  // this workspace, else a new minimized window, focus unchanged. Rust connects once its port is
  // ready; `null` when none is ready in time.
  const answerAgentFrameAsync = async (
    extensionId: string,
    requestId: string,
  ) => {
    const tabId = wm.openApp(extensionAppId(extensionId), null, true)
    if (tabId) wm.markTabMounted(tabId)
    const frame = tabId
      ? await readyFrames.waitFor(tabId, AGENT_FRAME_TIMEOUT_MS)
      : null
    await invoke('extension_agent_frame_ready', { requestId, frame })
  }
  keep(
    listen<{ extensionId: string; requestId: string }>(
      'extension-agent-frame',
      (event) =>
        void answerAgentFrameAsync(
          event.payload.extensionId,
          event.payload.requestId,
        ).catch((error: unknown) => {
          console.error('[extensions] a frame for the agent failed', error)
        }),
    ),
    'frame requests of the agent',
  )

  /** Loads the extension list; the session restore waits for it, so extension tabs survive. */
  async function startAsync(): Promise<void> {
    if (await reloadForDevModeAsync()) return
    await Promise.all([extensions.startAsync(), permissions.startAsync()])
  }

  return { startAsync }
}
