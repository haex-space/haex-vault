<script setup lang="ts">
/**
 * The first row of the file browser (spec 044 FR-003, FR-004): the sidebar button, back and forward,
 * the slot of the path bar (the panel's `PathBar.vue`, spec 062), list or grid, the sort and
 * whether hidden entries show.
 */
import { SORT_KEYS, type SortKey } from '~/lib/files/state'

const props = defineProps<{
  sidebarVisible: boolean
}>()

const emit = defineEmits<{
  toggleSidebar: []
}>()

const { t } = useI18n()
const router = useTabRouter()
const prefs = useFilesPrefs()

const sidebarLabel = computed(() =>
  props.sidebarVisible ? t('files.sidebar.hide') : t('files.sidebar.show'),
)

function pickSort(key: SortKey) {
  const current = prefs.sort.value
  prefs.setSort({
    key,
    ascending: current.key === key ? !current.ascending : true,
  })
}
</script>

<template>
  <div class="flex min-w-0 items-center gap-1">
    <UiButton
      variant="ghost"
      size="icon"
      class="shrink-0"
      :aria-label="sidebarLabel"
      :tooltip="sidebarLabel"
      :aria-expanded="sidebarVisible"
      aria-controls="files-sidebar"
      data-testid="files-sidebar-toggle"
      @click="emit('toggleSidebar')"
    >
      <Icon name="lucide:panel-left" class="size-4" />
    </UiButton>
    <UiButton
      variant="ghost"
      size="icon"
      class="shrink-0"
      :aria-label="t('files.back')"
      :tooltip="t('files.back')"
      :disabled="!router.canGoBack"
      data-testid="files-back"
      @click="router.back()"
    >
      <Icon name="lucide:arrow-left" class="size-4" />
    </UiButton>
    <UiButton
      variant="ghost"
      size="icon"
      class="shrink-0"
      :aria-label="t('files.forward')"
      :tooltip="t('files.forward')"
      :disabled="!router.canGoForward"
      data-testid="files-forward"
      @click="router.forward()"
    >
      <Icon name="lucide:arrow-right" class="size-4" />
    </UiButton>
    <slot />
    <UiButton
      variant="ghost"
      size="icon"
      class="shrink-0"
      :aria-label="
        prefs.view.value === 'list'
          ? t('files.view.grid')
          : t('files.view.list')
      "
      :tooltip="
        prefs.view.value === 'list'
          ? t('files.view.grid')
          : t('files.view.list')
      "
      data-testid="files-view-toggle"
      @click="prefs.setView(prefs.view.value === 'list' ? 'grid' : 'list')"
    >
      <Icon
        :name="
          prefs.view.value === 'list' ? 'lucide:layout-grid' : 'lucide:list'
        "
        class="size-4"
      />
    </UiButton>
    <ShadcnDropdownMenu>
      <ShadcnDropdownMenuTrigger as-child>
        <UiButton
          variant="ghost"
          size="icon"
          class="shrink-0"
          :aria-label="t('files.menu')"
          :tooltip="t('files.menu')"
          data-testid="files-menu"
        >
          <Icon name="lucide:arrow-down-up" class="size-4" />
        </UiButton>
      </ShadcnDropdownMenuTrigger>
      <ShadcnDropdownMenuContent align="end">
        <ShadcnDropdownMenuItem
          v-for="key in SORT_KEYS"
          :key="key"
          class="gap-2"
          :data-testid="`files-sort-${key}`"
          @select="pickSort(key)"
        >
          <Icon
            :name="
              prefs.sort.value.key !== key
                ? 'lucide:minus'
                : prefs.sort.value.ascending
                  ? 'lucide:arrow-up'
                  : 'lucide:arrow-down'
            "
            class="size-4"
          />
          {{ t(`files.sort.${key}`) }}
        </ShadcnDropdownMenuItem>
        <ShadcnDropdownMenuSeparator />
        <ShadcnDropdownMenuItem
          class="gap-2"
          data-testid="files-hidden-toggle"
          @select="prefs.setShowHidden(!prefs.showHidden.value)"
        >
          <Icon
            :name="prefs.showHidden.value ? 'lucide:eye-off' : 'lucide:eye'"
            class="size-4"
          />
          {{
            prefs.showHidden.value
              ? t('files.hidden.hide')
              : t('files.hidden.show')
          }}
        </ShadcnDropdownMenuItem>
      </ShadcnDropdownMenuContent>
    </ShadcnDropdownMenu>
  </div>
</template>
