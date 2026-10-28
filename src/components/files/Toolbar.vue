<script setup lang="ts">
/**
 * The first row of the file browser (spec 044 FR-003, FR-004): the sidebar button, the slot of the
 * bar of the panel (`PanelBar.vue`: back and forward, the path, list or grid, the sort; with two
 * panels each has its own, spec 062) and the switch for two panels.
 */
const props = defineProps<{
  sidebarVisible: boolean
  /** The switch for two panels exists (not on a phone, spec 062 FR-021). */
  splitAvailable: boolean
  splitOn: boolean
  /** The window is too narrow to turn two panels on (FR-022). */
  splitDisabled: boolean
}>()

const emit = defineEmits<{
  toggleSidebar: []
  toggleSplit: []
}>()

const { t } = useI18n()

const sidebarLabel = computed(() =>
  props.sidebarVisible ? t('files.sidebar.hide') : t('files.sidebar.show'),
)
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
    <slot />
    <UiButton
      v-if="splitAvailable"
      variant="ghost"
      size="icon"
      class="ml-auto shrink-0"
      :aria-label="t('files.split.toggle')"
      :tooltip="
        splitDisabled ? t('files.split.tooNarrow') : t('files.split.toggle')
      "
      :aria-pressed="splitOn"
      :disabled="splitDisabled"
      data-testid="files-split-toggle"
      @click="emit('toggleSplit')"
    >
      <Icon name="lucide:columns-2" class="size-4" />
    </UiButton>
  </div>
</template>
