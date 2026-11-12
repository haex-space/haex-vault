<script setup lang="ts">
/**
 * The settings frame (spec 023-settings-app, contracts §2), laid out like the COSMIC and GNOME
 * settings: the sidebar button and the search are in the window's header (spec 090), below it the
 * category sidebar and the content. The content starts with the location's title behind a back arrow
 * (sub-views) or the category's icon in the same slot (category start pages, so the title does
 * not shift); only the view below it scrolls. Title and view share one centered column, so the
 * list does not stretch across a wide window. Every view is a tab location (spec 020); the
 * registry in `lib/settings/registry.ts` supplies titles and hierarchy.
 *
 * The frame is a size container, so the sidebar follows the window's width, not the screen's
 * (FR-004): from `@2xl` it sits beside the content and can be hidden, below it is gone and opens
 * over the content (`useSidebarFrame`). Sidebar and search state are local to the open tab, nothing
 * is kept.
 */
import { toast } from 'vue-sonner'
import type { DeviceInfo } from '~/composables/useDevice'
import { SETTINGS_DEVICE_KEY } from '~/components/settings/deviceContext'
import {
  categoryOf,
  headerBack,
  locationFor,
  parentPathOf,
  SETTINGS_CATEGORIES,
} from '~/lib/settings/registry'
import { searchSettings } from '~/lib/settings/search'

const { t } = useI18n()
const { errString } = useErrorString()
const { currentDeviceInfoAsync } = useDevice()
const router = useTabRouter()
const tab = useWmTab()
const wm = useWindowManagerStore()

const root = useTemplateRef<HTMLElement>('root')
const sidebar = useTemplateRef<{ $el: HTMLElement; focusActive(): void }>(
  'sidebar',
)

const {
  wideHidden,
  menuOpen,
  wide,
  visible: sidebarVisible,
  isWide,
  show,
  hide,
} = useSidebarFrame(root, () => sidebar.value?.$el)

const sidebarClass = computed(() => [
  'absolute inset-0 z-20 bg-background px-2 pb-2 transition-[translate,width,margin,visibility] duration-200 ease-out motion-reduce:transition-none',
  '@2xl:static @2xl:z-auto @2xl:shrink-0 @2xl:overflow-hidden @2xl:bg-transparent @2xl:px-0 @2xl:translate-x-0',
  menuOpen.value ? 'visible translate-x-0' : 'invisible -translate-x-full',
  wideHidden.value
    ? '@2xl:invisible @2xl:ml-0 @2xl:w-0'
    : '@2xl:visible @2xl:ml-2 @2xl:w-64',
])

async function showSidebar(focusList: boolean) {
  show()
  if (!focusList) return
  await nextTick()
  sidebar.value?.focusActive()
}

function toggleSidebar() {
  if (sidebarVisible.value) hide()
  else void showSidebar(true)
}

function onSidebarEscape() {
  if (menuOpen.value && !isWide()) {
    menuOpen.value = false
    root.value
      ?.querySelector<HTMLElement>('[data-testid="settings-sidebar-toggle"]')
      ?.focus()
  }
}

const query = ref('')
/** `null` while no search runs; the hits appear in the sidebar, so typing shows it. */
const hits = computed(() =>
  query.value.trim() ? searchSettings(query.value, (key) => t(key)) : null,
)
watch(hits, (value) => {
  if (value && !sidebarVisible.value) void showSidebar(false)
})

function select(path: string) {
  router.push(path)
  query.value = ''
  menuOpen.value = false
}

function selectFirstHit() {
  const first = hits.value?.[0]
  if (first) select(first.path)
}

const sidebarLabel = computed(() =>
  sidebarVisible.value
    ? t('settings.sidebar.hide')
    : t('settings.sidebar.show'),
)

// The sidebar button and the search live in the window's header (spec 090, US1): always one open
// field, no row of its own below the title bar.
useWmHeader(
  () => ({
    start: [
      {
        type: 'toggle',
        id: 'sidebar',
        label: sidebarLabel.value,
        icon: 'lucide:panel-left',
        pressed: sidebarVisible.value,
        testId: 'settings-sidebar-toggle',
        collapse: { form: 'keep' },
      },
    ],
    center: [
      {
        type: 'search',
        id: 'q',
        label: t('settings.search.label'),
        placeholder: t('settings.search.placeholder'),
        value: query.value,
        collapse: { order: 10, form: 'shrink' },
      },
    ],
  }),
  (event) => {
    if (event.id === 'sidebar') toggleSidebar()
    else if (event.id === 'q' && event.kind === 'input')
      query.value = String(event.value ?? '')
    else if (event.id === 'q' && event.kind === 'submit') selectFirstHit()
  },
)

const deviceInfo = ref<DeviceInfo | null>(null)
const loadFailed = ref(false)

async function reloadDeviceInfoAsync() {
  loadFailed.value = false
  try {
    deviceInfo.value = await currentDeviceInfoAsync()
  } catch (error) {
    loadFailed.value = true
    toast.error(`${t('errors.deviceInfoFailed')}: ${errString(error)}`)
  }
}

provide(SETTINGS_DEVICE_KEY, {
  info: deviceInfo as Ref<DeviceInfo>,
  reloadAsync: reloadDeviceInfoAsync,
})

const current = computed(() => locationFor(router.route.path))
const title = computed(() =>
  current.value ? t(current.value.location.titleKey, current.value.params) : '',
)
const parentPath = computed(() =>
  current.value ? parentPathOf(current.value.location) : undefined,
)
const categoryIcon = computed(() => {
  const id = categoryOf(router.route.path)
  return SETTINGS_CATEGORIES.find((category) => category.id === id)?.icon
})
/** Recomputed on every navigation: the route path is read first, so the history is current. */
const back = computed(() => {
  if (!router.route.path || !tab.tabId) return undefined
  const history = wm.historyOf(tab.tabId)
  return history ? headerBack(history) : undefined
})
const backLabel = computed(() => {
  const target = back.value ? locationFor(back.value.path) : undefined
  return target
    ? t('settings.back', { title: t(target.location.titleKey, target.params) })
    : ''
})

function goBack() {
  const step = back.value
  if (!step) return
  if (step.kind === 'back') router.back()
  else router.push(step.path)
}

// The device's name is a vault row (`known_devices`) that can change on another device.
onVaultTablesChanged(['known_devices'], reloadDeviceInfoAsync)

onMounted(() => {
  void reloadDeviceInfoAsync()
})
</script>

<template>
  <div ref="root" class="@container flex h-full min-h-0 flex-col">
    <div class="relative flex min-h-0 flex-1 overflow-hidden">
      <SettingsSidebar
        id="settings-sidebar"
        ref="sidebar"
        :class="sidebarClass"
        :hits="hits"
        @select="select"
        @close="onSidebarEscape"
      />
      <main
        class="flex min-w-0 flex-1 flex-col"
        :inert="(menuOpen && !wide) || undefined"
      >
        <div class="shrink-0 px-6 pt-1 pb-3">
          <div class="mx-auto flex w-full max-w-3xl min-w-0 items-center gap-2">
            <UiButton
              v-if="parentPath"
              variant="ghost"
              size="icon"
              class="-ml-2 shrink-0"
              :aria-label="backLabel"
              :tooltip="backLabel"
              data-testid="settings-back"
              @click="goBack"
            >
              <Icon name="lucide:arrow-left" class="size-5" />
            </UiButton>
            <span
              v-else-if="categoryIcon"
              class="-ml-2 flex size-9 shrink-0 items-center justify-center text-muted-foreground"
              aria-hidden="true"
              data-testid="settings-category-icon"
            >
              <Icon :name="categoryIcon" class="size-5" />
            </span>
            <h1
              class="min-w-0 truncate text-2xl font-bold"
              data-testid="settings-title"
              :data-location="current?.location.id"
            >
              {{ title }}
            </h1>
          </div>
        </div>
        <div
          class="min-h-0 flex-1 overflow-y-auto px-6 pt-1 pb-6"
          :style="{ scrollPaddingBottom: '4rem' }"
        >
          <div class="mx-auto w-full max-w-3xl">
            <WmRouterView v-if="!loadFailed && deviceInfo" />
          </div>
        </div>
      </main>
    </div>
  </div>
</template>
