<template>
  <HaexSystem
    :is-dragging="isDragging"
    disable-content-scroll
  >
    <div class="h-full flex flex-col overflow-hidden">
      <HaexSystemPasswordsHeader
        v-if="viewMode === 'list'"
        class="flex-none"
      />
      <div class="flex-1 min-h-0 flex">
        <aside
          class="hidden @3xl:flex @3xl:flex-col shrink-0 overflow-y-auto border-e border-default"
          :style="{ width: `${sidebarWidth}px` }"
        >
          <HaexSystemPasswordsSidebar />
        </aside>
        <div
          class="hidden @3xl:block w-1 shrink-0 cursor-col-resize hover:bg-primary/40 active:bg-primary/60 transition-colors"
          :class="{ 'bg-primary/60': isResizing }"
          @mousedown="startResize"
          @dblclick="sidebarWidth = DEFAULT_SIDEBAR_WIDTH"
        />
        <main class="flex-1 min-w-0 overflow-hidden flex flex-col">
          <HaexSystemPasswordsEditor
            v-if="viewMode === 'item'"
            :key="selectedItemId ?? 'new'"
          />
          <template v-else>
            <!--
              Single slot for either breadcrumb or selection toolbar. They share
              the same 3rem height so the list below doesn't jump when selection
              mode kicks in. On desktop (@3xl+) the breadcrumb is hidden — the
              slot collapses unless the toolbar is active.
            -->
            <div
              v-if="showAccessorySlot"
              class="shrink-0 relative h-12"
              :class="{
                '@3xl:h-0': !isSelectionMode && !hasClipboard,
              }"
            >
              <Transition name="toolbar-swap">
                <HaexSystemPasswordsSelectionToolbar
                  v-if="isSelectionMode || hasClipboard"
                  key="toolbar"
                  class="absolute inset-x-0 top-0"
                  @tag="bulkTagOpen = true"
                  @delete="bulkDeleteOpen = true"
                  @paste="onPaste"
                  @edit-group="onEditGroupFromToolbar"
                />
                <HaexSystemPasswordsBreadcrumb
                  v-else-if="selectedGroupId !== null"
                  key="breadcrumb"
                  class="@3xl:hidden absolute inset-x-0 top-0"
                />
              </Transition>
            </div>
            <HaexSystemPasswordsList class="flex-1 min-h-0" />
          </template>
        </main>
      </div>
    </div>

    <HaexSystemPasswordsDialogBulkDelete
      v-model:open="bulkDeleteOpen"
      :entries="selectedEntries"
    />
    <HaexSystemPasswordsDialogBulkTag
      v-model:open="bulkTagOpen"
      :item-ids="selectedItemIds"
    />
    <HaexSystemPasswordsDialogGroupEditor
      v-model:open="groupEditorOpen"
      mode="edit"
      :group="groupUnderEdit"
    />
  </HaexSystem>
</template>

<script setup lang="ts">
const props = defineProps<{
  tabId?: string
  isDragging?: boolean
}>()

// Provide the tab ID so per-tab back/forward stacks stay isolated
// (same pattern the settings view uses).
provide('haex-tab-id', props.tabId ?? '')

const passwordsStore = usePasswordsStore()
const groupsStore = usePasswordsGroupsStore()
const selection = usePasswordsSelectionStore()
const { viewMode, selectedItemId } = storeToRefs(passwordsStore)
const { selectedGroupId } = storeToRefs(groupsStore)
const {
  selectedEntries,
  clipboardEntries,
  clipboardMode,
  isSelectionMode,
  hasClipboard,
} = storeToRefs(selection)

const showAccessorySlot = computed(
  () => isSelectionMode.value || hasClipboard.value || selectedGroupId.value !== null,
)
const toast = useToast()
const { t } = useI18n()

const selectedItemIds = computed(() =>
  selectedEntries.value.filter((e) => e.type === 'item').map((e) => e.id),
)

const bulkDeleteOpen = ref(false)
const bulkTagOpen = ref(false)
const groupEditorOpen = ref(false)
const groupUnderEdit = ref<null | ReturnType<typeof findGroup>>(null)

function findGroup(id: string) {
  return groupsStore.groupById.get(id) ?? null
}

const onEditGroupFromToolbar = (groupId: string) => {
  groupUnderEdit.value = findGroup(groupId)
  if (groupUnderEdit.value) groupEditorOpen.value = true
}

const onPaste = async () => {
  const entries = clipboardEntries.value.slice()
  const mode = clipboardMode.value
  if (!mode || entries.length === 0) return
  const target = selectedGroupId.value
  try {
    if (mode === 'cut') {
      await groupsStore.bulkMoveAsync(entries, target)
      toast.add({ title: t('toast.moved', { count: entries.length }), color: 'success' })
    } else {
      await groupsStore.bulkCloneAsync(entries, target)
      await passwordsStore.loadItemsAsync()
      toast.add({ title: t('toast.cloned', { count: entries.length }), color: 'success' })
    }
    selection.clearClipboard()
  } catch (error) {
    const key = error instanceof Error && error.message
    const title =
      key === 'cycleMove' || key === 'selfMove'
        ? t('toast.cycleError')
        : t('toast.pasteError')
    console.error('[Paste] failed:', error)
    toast.add({
      title,
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  }
}

// Leaving a view where the selection makes sense clears it — matches the
// file-manager convention where entering a different folder resets selection.
watch(selectedGroupId, () => selection.clear())
watch(viewMode, (mode) => {
  if (mode === 'item') selection.clear()
})

const { armWindowCloseBoundary } = usePasswordsNavigation(props.tabId ?? '')

onMounted(async () => {
  // Self-rearming sentinel — a back press from the list view is absorbed
  // instead of reaching the global close-window action.
  armWindowCloseBoundary()

  try {
    await Promise.all([
      passwordsStore.loadItemsAsync(),
      groupsStore.loadGroupsAsync(),
    ])
  } catch (error) {
    console.error('[Passwords] Failed to load items/groups:', error)
    toast.add({
      title: t('loadError'),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  }
})

const DEFAULT_SIDEBAR_WIDTH = 256
const MIN_SIDEBAR_WIDTH = 180
const MAX_SIDEBAR_WIDTH = 480

const sidebarWidth = ref(DEFAULT_SIDEBAR_WIDTH)
const isResizing = ref(false)

const startResize = (event: MouseEvent) => {
  event.preventDefault()
  const startX = event.clientX
  const startWidth = sidebarWidth.value
  isResizing.value = true

  const onMove = (e: MouseEvent) => {
    const next = startWidth + (e.clientX - startX)
    sidebarWidth.value = Math.min(
      MAX_SIDEBAR_WIDTH,
      Math.max(MIN_SIDEBAR_WIDTH, next),
    )
  }

  const onUp = () => {
    isResizing.value = false
    document.removeEventListener('mousemove', onMove)
    document.removeEventListener('mouseup', onUp)
  }

  document.addEventListener('mousemove', onMove)
  document.addEventListener('mouseup', onUp)
}
</script>

<i18n lang="yaml">
de:
  loadError: Passwörter konnten nicht geladen werden
  toast:
    moved: "{count} Einträge verschoben"
    cloned: "{count} Einträge dupliziert"
    pasteError: Einfügen fehlgeschlagen
    cycleError: Ziel-Ordner ist in der Auswahl enthalten
en:
  loadError: Failed to load passwords
  toast:
    moved: "{count} entries moved"
    cloned: "{count} entries duplicated"
    pasteError: Paste failed
    cycleError: Target folder is inside the selection
</i18n>

<style scoped>
/* Cross-fade + subtle slide so the toolbar swap with the breadcrumb reads as
   a transition rather than a pop. Both children are absolute-positioned in
   the same slot, so they overlap during the animation instead of reflowing. */
.toolbar-swap-enter-active,
.toolbar-swap-leave-active {
  transition:
    opacity 160ms ease,
    transform 160ms ease;
}
.toolbar-swap-enter-from {
  opacity: 0;
  transform: translateY(-4px);
}
.toolbar-swap-leave-to {
  opacity: 0;
  transform: translateY(4px);
}
</style>
