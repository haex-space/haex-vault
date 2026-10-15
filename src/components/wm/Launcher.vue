<script setup lang="ts">
/**
 * Lists every registered app with its localized name and icon; selecting
 * one opens it as a window (FR-002). An attention dot marks an app with a
 * tab waiting on a reply anywhere (any window, any workspace) — the
 * Launcher is one of the surfaces FR-015 requires that to reach even when
 * the affected window is elsewhere. Compact mode (T050) drops to two
 * columns so each tile stays a comfortable touch target on a narrow
 * screen, instead of three cramped ones. Spec 015-workspace-shell, T023,
 * T041, T050. Installed extensions are apps too (spec 017); one that cannot open on this
 * device is a disabled tile with the reason (T080). The last tile locks the vault (`useVaultLock`) — the one place
 * to lock now that the chat has no lock button. Right click or a long press on an app pins it to
 * the dock or unpins it (spec 045, FR-017).
 */
const open = defineModel<boolean>('open', { default: false })

const wm = useWindowManagerStore()
const { t } = useI18n()

const openApp = useAction('wm.app.open')
const { lock } = useVaultLock()
const dock = useDock()

function togglePin(appId: string) {
  void (dock.isPinned(appId) ? dock.unpinAsync(appId) : dock.pinAsync(appId))
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
      <div
        class="grid gap-3 overflow-x-hidden p-4"
        :class="wm.compact ? 'grid-cols-2' : 'grid-cols-3'"
      >
        <ShadcnContextMenu v-for="app in wm.apps()" :key="app.id">
          <ShadcnContextMenuTrigger as-child>
            <button
              type="button"
              class="relative flex flex-col items-center gap-2 rounded-lg p-3 text-sm text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:bg-transparent"
              :data-testid="app.id === 'system.chat' ? 'open-chat' : undefined"
              :data-app-id="app.id"
              :disabled="app.unavailableKey !== undefined"
              @click="launch(app.id)"
            >
              <span
                v-if="wm.appHasAttention(app.id)"
                class="absolute right-2 top-2 h-1.5 w-1.5 shrink-0 rounded-full bg-warning"
                :aria-label="t('wm.attention')"
              />
              <img
                v-if="app.iconUrl"
                :src="app.iconUrl"
                alt=""
                class="h-10 w-10 object-contain"
              />
              <Icon
                v-else
                :name="app.icon"
                class="h-6 w-6"
                :aria-hidden="true"
              />
              <span class="truncate">{{ app.title ?? t(app.titleKey) }}</span>
              <span
                v-if="app.unavailableKey"
                class="truncate text-xs text-muted-foreground"
              >
                {{ t(app.unavailableKey) }}
              </span>
            </button>
          </ShadcnContextMenuTrigger>
          <ShadcnContextMenuContent class="min-w-48">
            <ShadcnContextMenuItem
              data-testid="launcher-menu-pin"
              @select="togglePin(app.id)"
            >
              {{ t(dock.isPinned(app.id) ? 'wm.dock.unpin' : 'wm.dock.pin') }}
            </ShadcnContextMenuItem>
          </ShadcnContextMenuContent>
        </ShadcnContextMenu>
        <button
          type="button"
          class="flex flex-col items-center gap-2 rounded-lg p-3 text-sm text-foreground hover:bg-accent"
          data-testid="lock-instance"
          @click="lock"
        >
          <Icon
            name="lucide:lock-keyhole"
            class="h-6 w-6"
            :aria-hidden="true"
          />
          <span class="truncate">{{ t('wm.launcher.lock') }}</span>
        </button>
      </div>
    </template>
  </UiDrawerModal>
</template>
