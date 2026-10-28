<script setup lang="ts">
/**
 * The bar of one file browser panel (spec 044 FR-003, FR-004, spec 062): back and forward in the
 * panel's own history, the path (up, crumbs, reload), list or grid and the sort. With one panel it
 * stands in the toolbar, with two in the panel's own header, so each panel's controls act on that
 * panel only.
 */
import type { FilesView } from '~/composables/useFilesPrefs'
import { SORT_KEYS, type Sort, type SortKey } from '~/lib/files/state'

const props = defineProps<{
  path: string | null
  canGoUp: boolean
  /** The name the root shows in the path bar (a storage's name instead of `/`). */
  rootLabel?: string
  canGoBack: boolean
  canGoForward: boolean
  view: FilesView
  sort: Sort
  showHidden: boolean
}>()

const emit = defineEmits<{
  go: [path: string]
  up: []
  reload: []
  back: []
  forward: []
  view: [view: FilesView]
  sort: [sort: Sort]
  hidden: [show: boolean]
}>()

const { t } = useI18n()

const viewLabel = computed(() =>
  props.view === 'list' ? t('files.view.grid') : t('files.view.list'),
)

function pickSort(key: SortKey) {
  emit('sort', {
    key,
    ascending: props.sort.key === key ? !props.sort.ascending : true,
  })
}
</script>

<template>
  <div class="flex min-w-0 flex-1 items-center gap-1">
    <UiButton
      variant="ghost"
      size="icon"
      class="shrink-0"
      :aria-label="t('files.back')"
      :tooltip="t('files.back')"
      :disabled="!canGoBack"
      data-testid="files-back"
      @click="emit('back')"
    >
      <Icon name="lucide:arrow-left" class="size-4" />
    </UiButton>
    <UiButton
      variant="ghost"
      size="icon"
      class="shrink-0"
      :aria-label="t('files.forward')"
      :tooltip="t('files.forward')"
      :disabled="!canGoForward"
      data-testid="files-forward"
      @click="emit('forward')"
    >
      <Icon name="lucide:arrow-right" class="size-4" />
    </UiButton>
    <FilesPathBar
      :path="path"
      :root-label="rootLabel"
      :can-go-up="canGoUp"
      @go="emit('go', $event)"
      @up="emit('up')"
      @reload="emit('reload')"
    />
    <UiButton
      variant="ghost"
      size="icon"
      class="shrink-0"
      :aria-label="viewLabel"
      :tooltip="viewLabel"
      data-testid="files-view-toggle"
      @click="emit('view', view === 'list' ? 'grid' : 'list')"
    >
      <Icon
        :name="view === 'list' ? 'lucide:layout-grid' : 'lucide:list'"
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
              sort.key !== key
                ? 'lucide:minus'
                : sort.ascending
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
          @select="emit('hidden', !showHidden)"
        >
          <Icon
            :name="showHidden ? 'lucide:eye-off' : 'lucide:eye'"
            class="size-4"
          />
          {{ showHidden ? t('files.hidden.hide') : t('files.hidden.show') }}
        </ShadcnDropdownMenuItem>
      </ShadcnDropdownMenuContent>
    </ShadcnDropdownMenu>
  </div>
</template>
