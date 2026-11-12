<script setup lang="ts">
/**
 * Shows every registered app as a tile in a scrolling grid (`wm/AppTiles.vue`, shared with the
 * "+" of a window, spec 091); selecting one opens it as a window (FR-002). An attention dot marks
 * an app with a tab waiting on a reply anywhere (any window, any workspace) — the Launcher is one
 * of the surfaces FR-015 requires that to reach even when the affected window is elsewhere.
 * Compact mode (T050) keeps at most two columns so each tile stays a comfortable touch target on a
 * narrow screen, instead of three cramped ones (`gridColumns`). Spec 015-workspace-shell, T023,
 * T041, T050. Installed extensions are apps too (spec 017); one that cannot open on this device is
 * an unavailable tile with the reason (T080). The strip below the grid (outside the scrolling
 * tiles, so always in reach) locks the vault (`useVaultLock`) — the one place to lock now that the
 * chat has no lock button. Right click or a long press on an app pins it to the dock or unpins it
 * (spec 045, FR-017). A button in the same strip switches the voice assistant on or off (spec 066
 * FR-001); when it stays off because something is missing, the settings open and the launcher
 * closes.
 */
import { PHASE_ICONS } from '~/lib/assistant/indicator'

const open = defineModel<boolean>('open', { default: false })

const wm = useWindowManagerStore()
const { t } = useI18n()

const openApp = useAction('wm.app.open')
const { lock } = useVaultLock()
const assistant = useAssistantStore()
const toggleAssistant = useAction('assistant.toggle')
void assistant.start()

/** Switches the assistant; when it could not be switched on (the settings opened, or the notice
 * was not confirmed), the launcher gets out of the way. */
async function toggleAssistantAsync() {
  const wasOn = assistant.state.enabled
  await toggleAssistant({})
  if (!wasOn && !assistant.state.enabled) open.value = false
}

function launch(appId: string) {
  void openApp({ appId })
  open.value = false
}
</script>

<template>
  <UiDrawerModal
    :open="open"
    :title="t('wm.launcher.title')"
    @update:open="open = $event"
  >
    <template #content>
      <WmAppTiles :apps="wm.apps()" pin-menu @pick="launch" />
    </template>
    <!-- Not apps: they stay in the footer strip, outside the scrolling grid, so locking is always
         reachable (spec 091 FR-022). -->
    <template #footer>
      <div class="flex w-full flex-wrap items-center justify-center gap-2">
        <button
          v-if="assistant.available"
          type="button"
          class="flex items-center gap-2 rounded-lg px-3 py-2 text-sm hover:bg-accent"
          :class="
            assistant.state.enabled
              ? 'bg-primary/10 text-primary'
              : 'text-foreground'
          "
          :aria-pressed="assistant.state.enabled"
          data-testid="launcher-assistant-toggle"
          @click="toggleAssistantAsync"
        >
          <Icon
            :name="PHASE_ICONS[assistant.state.phase]"
            class="h-5 w-5"
            :aria-hidden="true"
          />
          <span class="truncate">{{
            t(assistant.state.enabled ? 'assistant.on' : 'assistant.off')
          }}</span>
        </button>
        <button
          type="button"
          class="flex items-center gap-2 rounded-lg px-3 py-2 text-sm text-foreground hover:bg-accent"
          data-testid="lock-instance"
          @click="lock"
        >
          <Icon
            name="lucide:lock-keyhole"
            class="h-5 w-5"
            :aria-hidden="true"
          />
          <span class="truncate">{{ t('wm.launcher.lock') }}</span>
        </button>
      </div>
    </template>
  </UiDrawerModal>
</template>
