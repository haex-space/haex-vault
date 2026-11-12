<script setup lang="ts">
/**
 * One tab's content: its own `useWmTab()` context (spec
 * 015-workspace-shell, T034) — a *different* component instance per tab, so
 * Vue's provide/inject correctly scopes `tabId`/attention/guard to just
 * this tab, even while other tabs in the same window stay mounted
 * alongside it (research R8). Hidden via `v-show`, not `v-if`, when
 * inactive; lazy-mounted the first time it becomes active
 * (`wm.markTabMounted`), then left mounted. Spec 020: the content is
 * the tab's root `WmRouterView`, which renders the app for the tab's
 * current location; an extension's tab (spec 017) shows its sandboxed frame instead.
 */
import { computed, watchEffect } from 'vue'
import { getAppRoutes } from '~/components/wm/appRoutes'
import { provideWmTab } from '~/composables/useWmTab'
import { extensionIdOf } from '~/lib/extensions/apps'
import type { WmTab } from '~/lib/wm/types'

const props = defineProps<{
  tab: WmTab
  windowId: string
  active: boolean
}>()

const wm = useWindowManagerStore()

const hasRoutes = computed(() => getAppRoutes(props.tab.appId) !== undefined)
// Spec 017: an extension's tab shows its frame instead of holzi routes.
const extensionId = extensionIdOf(props.tab.appId)
const mounted = computed(() => wm.runtimeFor(props.tab.id).mounted)

provideWmTab({
  tabId: props.tab.id,
  windowId: props.windowId,
  appId: props.tab.appId,
  requestAttention: () => wm.setTabAttention(props.tab.id, true),
  clearAttention: () => wm.setTabAttention(props.tab.id, false),
  setTitle: (value) => wm.setTabTitle(props.tab.id, value),
  registerCloseGuard: (guard) => {
    wm.setTabCloseGuard(props.tab.id, guard)
    return () => wm.setTabCloseGuard(props.tab.id, null)
  },
  // action-exempt: the window manager↔App contract's programmatic API (closeSelf, openApp) — apps call it
  // from their own logic, not as a user control.
  closeSelf: () => wm.closeTab(props.windowId, props.tab.id),
  // action-exempt: part of the same programmatic app API as closeSelf above.
  openApp: (appId, at) => {
    wm.openApp(appId, at ?? null)
  },
  registerActionHandler: (actionId, handler) =>
    wm.registerTabActionHandler(props.tab.id, actionId, handler),
  setHeader: (header) => wm.setTabHeader(props.tab.id, header),
  registerHeaderHandler: (handler) =>
    wm.registerTabHeaderHandler(props.tab.id, handler),
})

watchEffect(() => {
  if (props.active) wm.markTabMounted(props.tab.id)
})
</script>

<template>
  <div
    v-show="active"
    role="tabpanel"
    class="h-full min-h-0"
    :aria-hidden="!active"
  >
    <ExtensionsExtensionFrame
      v-if="extensionId !== null && mounted"
      :extension-id="extensionId"
    />
    <WmRouterView v-else-if="hasRoutes && mounted" />
  </div>
</template>
