<template>
  <HaexSystem
    :is-dragging="isDragging"
    disable-content-scroll
  >
    <div
      ref="containerRef"
      class="h-full flex flex-col overflow-hidden"
    >
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
            <!-- Toolbar fades over breadcrumbs in the same row — no layout jump. -->
            <div class="relative shrink-0">
              <HaexSystemPasswordsBreadcrumb />
              <Transition name="toolbar-fade">
                <HaexSystemPasswordsSelectionToolbar
                  v-if="isSelectionMode || hasClipboard"
                  :class="selectedGroupId !== null ? 'absolute inset-0' : undefined"
                  @tag="bulkTagOpen = true"
                  @delete="bulkDeleteOpen = true"
                  @paste="onPaste"
                  @edit-group="onEditGroupFromToolbar"
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
      :final="isBulkDeleteFinal"
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
import { useResizeObserver } from '@vueuse/core'

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
const { viewMode, selectedItemId, items } = storeToRefs(passwordsStore)
const { selectedGroupId } = storeToRefs(groupsStore)
const {
  selectedEntries,
  selectedCount,
  clipboardEntries,
  clipboardMode,
  isSelectionMode,
  hasClipboard,
  desktopFocusId,
} = storeToRefs(selection)

// ── Wide-layout detection ────────────────────────────────────────────────────
// The sidebar becomes visible at @3xl = 48rem (768px) on the @container root.
// On wide layout, single-click selects instead of opening — same mental model
// as a desktop file manager.
const containerRef = useTemplateRef<HTMLElement>('containerRef')
const isWideLayout = ref(false)
useResizeObserver(containerRef, (entries) => {
  const entry = entries[0]
  if (entry) isWideLayout.value = entry.contentRect.width >= 768
})
provide('passwords:isWideLayout', isWideLayout)

// ── Visible list ─────────────────────────────────────────────────────────────
// Computed here so both the list component and the selection toolbar share the
// same ordered IDs without double-computing them.
const { visibleOrderedIds } = usePasswordsVisibleList()
provide('passwords:visibleOrderedIds', visibleOrderedIds)

const toast = useToast()
const { t } = useI18n()

const selectedItemIds = computed(() =>
  selectedEntries.value.filter((e) => e.type === 'item').map((e) => e.id),
)

const isBulkDeleteFinal = computed(() => {
  const groupId = selectedGroupId.value
  if (!groupId) return false
  return groupsStore.isGroupInTrash(groupId)
})

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

// ── Keyboard shortcuts ───────────────────────────────────────────────────────

const isInputFocused = () => {
  const el = document.activeElement
  if (!el) return false
  const tag = el.tagName
  return tag === 'INPUT' || tag === 'TEXTAREA' || (el as HTMLElement).isContentEditable
}

const onKeydown = (event: KeyboardEvent) => {
  if (viewMode.value !== 'list') return
  const ctrl = event.ctrlKey || event.metaKey

  // Ctrl+A — select all visible entries in the current folder/search result.
  if (ctrl && event.key === 'a' && !isInputFocused()) {
    event.preventDefault()
    selection.selectAll(visibleOrderedIds.value)
    return
  }

  // Ctrl+B / Ctrl+C — copy username / password for the desktop-focused item
  // or the single selected item. Only active on wide layout.
  if (ctrl && isWideLayout.value) {
    const targetId = desktopFocusId.value
      ?? (selectedCount.value === 1 ? selectedEntries.value[0]?.id : null)
    if (!targetId) return
    const focusedEntry = selectedEntries.value.find((e) => e.id === targetId)
    if (focusedEntry && focusedEntry.type !== 'item') return
    const item = items.value.find((i) => i.id === targetId)
    if (!item) return

    if (event.key === 'b') {
      event.preventDefault()
      if (item.username) {
        navigator.clipboard.writeText(item.username).then(() => {
          toast.add({ title: t('toast.usernameCopied'), color: 'success', duration: 1500 })
        })
      }
      return
    }

    if (event.key === 'c' && !window.getSelection()?.toString()) {
      event.preventDefault()
      if (item.password) {
        navigator.clipboard.writeText(item.password).then(() => {
          toast.add({ title: t('toast.passwordCopied'), color: 'success', duration: 1500 })
        })
      }
    }
  }
}

onMounted(() => {
  window.addEventListener('keydown', onKeydown)
})

onUnmounted(() => {
  window.removeEventListener('keydown', onKeydown)
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
    passwordCopied: Passwort kopiert
    usernameCopied: Benutzername kopiert
en:
  loadError: Failed to load passwords
  toast:
    moved: "{count} entries moved"
    cloned: "{count} entries duplicated"
    pasteError: Paste failed
    cycleError: Target folder is inside the selection
    passwordCopied: Password copied
    usernameCopied: Username copied
</i18n>

<style scoped>
/* Toolbar fades over the breadcrumb row (same absolute slot), so there is no
   layout shift and no slide — pure opacity crossfade is enough. */
.toolbar-fade-enter-active,
.toolbar-fade-leave-active {
  transition: opacity 160ms ease;
}
.toolbar-fade-enter-from,
.toolbar-fade-leave-to {
  opacity: 0;
}
</style>
