<script setup lang="ts">
/**
 * Lists every window of the active workspace — including minimized ones —
 * with icon, active-tab title, tab count and an attention badge; selecting
 * one restores/focuses it, a second control closes it (FR-011, FR-015).
 * The close button grows to a full touch target in compact mode (T050) —
 * at its normal size the icon-only button falls well short of one.
 * Spec 015-workspace-shell, T030, T050.
 */
import { computed } from 'vue'
import type { WmWindow } from '~/lib/wm/types'

const open = defineModel<boolean>('open', { default: false })

const wm = useWindowManagerStore()
const { t } = useI18n()

type Row = {
  win: WmWindow
  info: NonNullable<ReturnType<typeof wm.windowDisplayInfo>>
}

const rows = computed<Row[]>(() => {
  const result: Row[] = []
  for (const win of wm.windowsInActiveWorkspace) {
    const info = wm.windowDisplayInfo(win)
    if (info) result.push({ win, info })
  }
  return result
})

/** Restores and focuses the chosen window, then closes the overview. */
const focusWindow = useAction('wm.window.focus')
const closeWindow = useAction('wm.window.close')

function select(windowId: string) {
  void focusWindow({ windowId })
  open.value = false
}
</script>

<template>
  <UiDrawerModal
    :open="open"
    :title="t('wm.windowOverview.title')"
    @update:open="open = $event"
  >
    <template #content>
      <ul class="flex flex-col gap-1 overflow-x-hidden p-2">
        <li
          v-for="row in rows"
          :key="row.win.id"
          class="flex items-center gap-1 rounded-lg hover:bg-accent"
        >
          <button
            type="button"
            class="flex min-w-0 flex-1 items-center gap-2 rounded p-2 text-left text-sm"
            @click="select(row.win.id)"
          >
            <WmTabIcon
              :icon="row.info.icon"
              :icon-url="row.info.iconUrl"
              class="h-4 w-4 shrink-0"
            />
            <span class="min-w-0 flex-1 truncate">{{
              row.info.titleOverride ??
              (row.info.titleKey
                ? t(row.info.titleKey, row.info.titleParams)
                : '')
            }}</span>
            <span
              v-if="row.info.tabCount > 1"
              class="shrink-0 text-xs text-muted-foreground"
            >
              {{ row.info.tabCount }}
            </span>
            <span
              v-if="row.info.hasAttention"
              class="h-1.5 w-1.5 shrink-0 rounded-full bg-warning"
              :aria-label="t('wm.attention')"
            />
          </button>
          <button
            type="button"
            class="shrink-0 rounded text-muted-foreground hover:bg-accent hover:text-foreground"
            :class="wm.compact ? 'p-3.5' : 'p-1.5'"
            :aria-label="t('wm.window.close')"
            @click="closeWindow({ windowId: row.win.id })"
          >
            <Icon name="lucide:x" class="h-3.5 w-3.5" :aria-hidden="true" />
          </button>
        </li>
        <li v-if="rows.length === 0" class="p-2 text-xs text-muted-foreground">
          {{ t('wm.windowOverview.empty') }}
        </li>
      </ul>
    </template>
  </UiDrawerModal>
</template>
