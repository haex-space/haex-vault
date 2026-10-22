<script setup lang="ts">
/**
 * The Chevron dropdown (FR-034): every tab of the window, in bar order,
 * with icon, title, a checkmark on the active one, and an attention badge —
 * selecting one activates it (`wm/TabBar.vue`'s own watch then scrolls it
 * into view, FR-035). Spec 015-workspace-shell, T036, plan research R18.
 */
import { computed } from 'vue'
import type { WmTab } from '~/lib/wm/types'

const props = defineProps<{
  tabs: WmTab[]
  activeTabId: string
}>()

const emit = defineEmits<{
  selectTab: [tabId: string]
}>()

const wm = useWindowManagerStore()
const { t } = useI18n()

function titleFrom(info: ReturnType<typeof wm.tabDisplayInfo>): string {
  return (
    info.titleOverride ??
    (info.titleKey ? t(info.titleKey, info.titleParams) : '')
  )
}

const rows = computed(() =>
  props.tabs.map((tab) => ({ tab, info: wm.tabDisplayInfo(tab) })),
)
</script>

<template>
  <ShadcnDropdownMenu>
    <ShadcnDropdownMenuTrigger as-child>
      <button
        type="button"
        class="shrink-0 rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
        :aria-label="t('wm.tabs.listMenu')"
        @pointerdown.stop
      >
        <Icon
          name="lucide:chevron-down"
          class="h-3.5 w-3.5"
          :aria-hidden="true"
        />
      </button>
    </ShadcnDropdownMenuTrigger>
    <ShadcnDropdownMenuContent align="end">
      <ShadcnDropdownMenuItem
        v-for="row in rows"
        :key="row.tab.id"
        class="gap-2"
        @select="emit('selectTab', row.tab.id)"
      >
        <WmTabIcon
          :icon="row.info.icon"
          :icon-url="row.info.iconUrl"
          class="h-4 w-4 shrink-0"
        />
        <span class="min-w-0 max-w-48 truncate">{{ titleFrom(row.info) }}</span>
        <Icon
          v-if="row.tab.id === activeTabId"
          name="lucide:check"
          class="h-3.5 w-3.5 shrink-0"
          :aria-hidden="true"
        />
        <span
          v-if="row.info.hasAttention"
          class="ml-auto h-1.5 w-1.5 shrink-0 rounded-full bg-warning"
          :aria-label="t('wm.attention')"
        />
      </ShadcnDropdownMenuItem>
    </ShadcnDropdownMenuContent>
  </ShadcnDropdownMenu>
</template>
