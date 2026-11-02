<script setup lang="ts">
/**
 * The window manager's desktop area: every window of the active workspace, plus the dock with the
 * Launcher, window-overview and workspace-overview entries (spec 015-workspace-shell, T023, T030,
 * T040, T049, FR-002, FR-011, FR-022; spec 045, FR-001).
 *
 * `wm.area` is the size of the element the windows live in — smaller than the app window when the
 * dock reserves space — while `wm.compact` follows the app window's width (spec 045 research R1,
 * FR-033), so the dock moving in compact mode cannot switch compact mode back. The store's initial
 * area is the app window's size, so only actual *changes* need forwarding; `updateArea` itself
 * re-clamps window geometry into the new area (`layoutState.ts`), `wm/Window.vue`'s
 * `windowDisplayRect` already handles the compact/normal display switch without this component's
 * help.
 * The overlay open states live in the store (`wm.overlays`, spec 020) so
 * system back can close them and open the window overview.
 *
 * Behind every workspace lies the vault's background image, if one is set (spec 042, FR-017).
 * While an agent steers the agent browser, the sessions bar lies over the windows (spec 065,
 * FR-015).
 */
import { computed, useTemplateRef, watch } from 'vue'

const wm = useWindowManagerStore()
const { width: viewportWidth } = useWindowSize()
const windowArea = useTemplateRef<HTMLElement>('windowArea')
const { width, height } = useElementSize(windowArea)

watch([width, height, viewportWidth], ([newWidth, newHeight, newViewport]) => {
  // Not laid out yet (or hidden): there is no area to clamp windows into.
  if (newWidth === 0 || newHeight === 0) return
  wm.updateArea({ width: newWidth, height: newHeight }, newViewport)
})

// A dock reserving space sits beside the window area on its edge (spec 045, FR-024).
const dock = useDock()
const DIRECTION = {
  top: 'flex-col-reverse',
  bottom: 'flex-col',
  left: 'flex-row-reverse',
  right: 'flex-row',
} as const
const direction = computed(() => DIRECTION[dock.effective.value.edge])

const { background } = useWorkspaceBackground()
// The value is a checked WebP data URL (`isBackgroundValue`): only base64 inside the quotes.
const backgroundStyle = computed(() =>
  background.value
    ? {
        backgroundImage: `url("${background.value}")`,
        backgroundSize: 'cover',
        backgroundPosition: 'center',
      }
    : undefined,
)
</script>

<template>
  <div
    class="relative flex h-full min-h-0 w-full overflow-hidden bg-muted/10"
    :class="direction"
    :style="backgroundStyle"
    data-testid="wm-desktop"
  >
    <div ref="windowArea" class="relative isolate min-h-0 min-w-0 flex-1">
      <WmWindow
        v-for="win in wm.windowsInActiveWorkspace"
        :key="win.id"
        :window="win"
        :active="win.id === wm.activeWindowId"
      />
      <AgentBrowserSessionsBar />
    </div>

    <WmDock />

    <WmLauncher v-model:open="wm.overlays.launcher" />
    <WmWindowOverview v-model:open="wm.overlays.windows" />
    <WmWorkspaceOverview v-model:open="wm.overlays.workspaces" />
    <WmCloseConfirm />
    <ExtensionsPermissionRequestDialog />
    <ExtensionsPeerInvitationDialog />
    <BrowserBridgePrompts />
    <AgentsPrompts />
    <ExtensionsEncryptedFolderChooser />
    <StorageCredentialsModal />
  </div>
</template>
