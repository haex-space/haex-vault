<script setup lang="ts">
/**
 * window manager host page (spec 015-workspace-shell, T023). Replaces the spec-002
 * workspace stub: onboarding enforcement (FR-001) stays here since this is
 * now the only real page apps are reached through (chat/settings/
 * federation moved into window manager apps, T021-T022). The model status shows in
 * the chat only; the workspace-wide status bar (015 FR-005) was dropped by
 * operator decision.
 *
 * Awaits `wm.restoreSessionAsync()` (spec 022) before anything else: with
 * the setting "Sitzung wiederherstellen" on, the saved session replaces the
 * store's initial empty workspace; an app opened before that (the launcher is
 * already there) is opened again in the restored session. A failure is logged and the page carries on with the empty
 * start, so a deep link below still opens its app (spec 022 FR-012, FR-014).
 *
 * Consumes `?open=<appId>` (and spec 020's optional `&at=<path>`) once (contracts/shell-app-contract.md §3, T024's
 * legacy-route redirects land here with it set) and removes it via
 * `router.replace` so it does not re-fire and open a second tab on a
 * later navigation that happens to keep it in the URL. A query that arrives while the page is
 * already mounted (a `router.replace` to this route) is consumed the same way, once the session
 * is restored (spec 023, quickstart S8–S10).
 */
definePageMeta({
  middleware: ['onboarded'],
})

const route = useRoute()
const router = useRouter()
const instancesStore = useInstancesStore()
const wm = useWindowManagerStore()
const colorScheme = useColorScheme()
const appearance = useAppearance()
const language = useLanguage()
const background = useWorkspaceBackground()
const dock = useDock()

// The permission dialog must have its event listener before Rust can publish an agent request.
let resolvePermissionDialogReady!: () => void
const permissionDialogReady = new Promise<void>((resolve) => {
  resolvePermissionDialogReady = resolve
})

// The color scheme is a vault preference: another device or window can change it.
onVaultTablesChanged(['preferences'], colorScheme.refreshAsync)
// The appearance (spec 035) is a vault preference too.
onVaultTablesChanged(['preferences'], appearance.refreshAsync)
// And the language (spec 042, FR-010): changed on another device, it switches here too.
onVaultTablesChanged(['preferences'], language.refreshAsync)
// And the workspace background (spec 042, FR-018).
onVaultTablesChanged(['preferences'], background.refreshAsync)
// And the dock's entries (spec 045, FR-040); its placement is this device's, but lives in the same table.
onVaultTablesChanged(['preferences'], dock.refreshAsync)
// So is "Sitzung wiederherstellen" (spec 023 FR-024): turned on on another device, this one starts
// saving its session at once.
onVaultTablesChanged(['preferences'], wm.refreshSessionRestoreAsync)
// Spec 020: create the models store here, inside a component setup (its setup calls `useI18n()`),
// so the global chat actions (`stores/chatActionHandlers.ts`) find it when an action runs.
useModelsStore()

// Spec 020: global shortcuts for window manager actions (back/forward).
useWmKeyboard()

// Spec 032: the actions a model in the chat may call (set up in `onMounted` once the vault is open).
const agentActions = useAgentActions()

// Spec 017: installed extensions are apps; their tabs are restored with the session.
const extensionHost = useExtensionHost()

// Spec 020 (research R7, FR-020, FR-035): the webview history is never navigation state. Pages
// reach this one with `replace`, so the top document's history stays flat, and every router
// navigation away from it — a webview back, or an embedded document's `history.back()` — is
// cancelled and deliberately not read as a tab's back (locking or closing ends the process,
// spec 013, so there is no legitimate route away).
onBeforeRouteLeave(() => false)

const instanceName = computed(() => {
  const raw = route.params.instance
  return typeof raw === 'string'
    ? raw
    : Array.isArray(raw)
      ? (raw[0] ?? '')
      : ''
})

// A deep link is consumed once the saved session is in place (spec 022 FR-012), also when the
// query arrives later.
const sessionRestored = ref(false)
watch([sessionRestored, () => route.query.open], ([restored]) => {
  if (restored) {
    void consumeDeepLink().catch((error: unknown) => {
      console.error('[wm] consuming the deep link failed', error)
    })
  }
})

// Spec 060 FR-018a: a link of a share opens the reading-in page as a preview, once the saved
// session is in place; a link that arrives later opens it the same way. The text stays in the
// inbox store for the page, never in the address.
const shareInbox = useShareInboxStore()
watch([sessionRestored, () => shareInbox.text], ([restored, text]) => {
  if (restored && text !== null) {
    void wm
      .runAction('wm.app.open', { appId: 'contacts', at: '/import' })
      .then((outcome) => {
        if (!outcome.ok) console.error('[wm] opening the share failed', outcome)
      })
  }
})

// Normally already set by the caller (pages/index.vue) before navigating
// here; set again so a direct/refreshed load of this route still resolves
// the same instance for components that only read the store (ChatApp.vue
// and friends have no route of their own).
onMounted(async () => {
  instancesStore.setActiveInstance(instanceName.value)
  // Spec 023 (FR-014): the vault's color scheme; a read error leaves the system's. Spec 035: the
  // vault's appearance, a read error leaves the defaults. Both are started before the session is
  // restored, so the windows are not first painted in the default colors.
  void colorScheme.loadAsync().catch((error: unknown) => {
    console.error('[settings] reading the color scheme failed', error)
  })
  void appearance.loadAsync().catch((error: unknown) => {
    console.error('[settings] reading the appearance failed', error)
  })
  // Spec 042: normally applied by the start page already; again for a direct or reloaded load.
  void language.loadAsync().catch((error: unknown) => {
    console.error('[settings] reading the language failed', error)
  })
  void background.loadAsync().catch((error: unknown) => {
    console.error('[settings] reading the background failed', error)
  })
  // Spec 045: a read error leaves the default dock.
  void dock.loadAsync().catch((error: unknown) => {
    console.error('[dock] reading the dock failed', error)
  })
  // The extension list first: a restored tab of an extension unknown at that moment is dropped.
  await extensionHost.startAsync().catch((error: unknown) => {
    console.error('[extensions] loading the extension list failed', error)
  })
  try {
    await wm.restoreSessionAsync()
  } catch (error) {
    console.error('[wm] restoring the session failed; starting empty', error)
  }
  void useModelDownloadsStore()
    .watchDownloads()
    .catch((error: unknown) => {
      console.error('[models] watching download progress failed', error)
    })
  // Spec 024 (FR-032): changes to the vault's data, whoever made them, must appear in open
  // windows and tabs without reloading. Started once per vault session, never torn down — same
  // lifecycle as the subscription above.
  void startVaultDataListening().catch((error: unknown) => {
    console.error('[vault-data] listening for data changes failed', error)
  })

  // Spec 032 (ADR-0006): offer the actions to the built-in agent. Started once per vault session,
  // like the sync listener; without it the chat simply has no holzi tools. Wait for the initial
  // registration so the first chat turn cannot race the action list being installed in Rust.
  try {
    await permissionDialogReady
    await agentActions.startAsync()
  } catch (error: unknown) {
    console.error('[agent] offering the actions to the model failed', error)
  }

  sessionRestored.value = true
})

async function consumeDeepLink(): Promise<void> {
  const open = route.query.open
  if (typeof open !== 'string' || open.length === 0) return
  const at = route.query.at
  const { open: _open, at: _at, ...rest } = route.query
  // Remove the transport query before opening the app. Awaiting this replace
  // keeps the address bar and the window-manager state in sync under slow CI
  // webviews; otherwise the E2E check can observe the app before the query is
  // gone and the deep link may be replayed by a later navigation.
  await router.replace({ query: rest })

  // Spec 020 FR-012/FR-013: `&at=<path>` opens the app at a location.
  const outcome = await wm.runAction('wm.app.open', {
    appId: open,
    ...(typeof at === 'string' && at.length > 0 ? { at } : {}),
  })
  if (!outcome.ok) {
    console.error('[wm] deep link action failed', outcome)
  }
}
</script>

<template>
  <div
    class="holzi-safe-area flex h-screen min-h-0 flex-col"
    :class="{ 'holzi-keyboard-room': wm.compact }"
  >
    <SyncCopyNotice />
    <FilesAgentPermissionDialog @ready="resolvePermissionDialogReady" />
    <FilesShareConfirmDialog />
    <FilesSyncRuleConfirmDialog />
    <ContactsShareConfirmDialog />
    <MailLinkDialog :ready="sessionRestored" />
    <WmDesktop class="min-h-0 flex-1" />
  </div>
</template>
