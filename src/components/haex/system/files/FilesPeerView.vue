<template>
  <div class="flex flex-col gap-4 h-full">
    <!-- Loading -->
    <div
      v-if="browser.isLoading.value"
      class="flex items-center justify-center py-16"
    >
      <UIcon
        name="i-lucide-loader-2"
        class="w-8 h-8 animate-spin text-muted"
      />
    </div>

    <!-- Error -->
    <div
      v-else-if="browser.loadError.value"
      class="flex flex-col items-center justify-center py-16 gap-3"
    >
      <UIcon
        name="i-lucide-alert-circle"
        class="w-8 h-8 text-error"
      />
      <p class="text-sm text-error">{{ browser.loadError.value }}</p>
      <UiButton
        variant="ghost"
        icon="i-lucide-refresh-cw"
        @click="browser.loadFiles()"
      >
        {{ t('retry') }}
      </UiButton>
    </div>

    <!-- Empty folder / no results / still searching -->
    <div
      v-else-if="browser.filteredFiles.value.length === 0"
      class="text-center py-16"
    >
      <template v-if="browser.isSearching.value">
        <UIcon
          name="i-lucide-loader-2"
          class="w-8 h-8 mx-auto mb-2 animate-spin text-muted"
        />
        <p class="text-muted">{{ t('searching') }}</p>
      </template>
      <template v-else>
        <UIcon
          :name="
            browser.searchQuery.value
              ? 'i-lucide-search-x'
              : 'i-lucide-folder-open'
          "
          class="w-12 h-12 mx-auto mb-2 opacity-30"
        />
        <p class="text-muted">
          {{
            browser.searchQuery.value ? t('noResults') : t('emptyFolder')
          }}
        </p>
      </template>
    </div>

    <!-- File listing -->
    <div v-else>
      <!-- Select all / Back row -->
      <div class="flex items-center gap-3 p-3">
        <UCheckbox
          :model-value="browser.allSelected.value"
          @update:model-value="
            browser.allSelected.value
              ? browser.clearSelection()
              : browser.selectAll()
          "
        />
        <div
          v-if="browser.currentPath.value !== '/'"
          class="flex items-center gap-2 cursor-pointer hover:text-primary transition-colors"
          @click="browser.navigateUp()"
        >
          <UIcon
            name="i-lucide-arrow-up"
            class="w-4 h-4 text-muted"
          />
          <span class="text-sm text-muted">..</span>
        </div>
        <span
          v-else
          class="text-xs text-muted"
        >
          {{ t('selectAll') }}
        </span>
      </div>

      <!-- ===== List view ===== -->
      <div
        v-if="browser.viewMode.value === 'list'"
        class="space-y-1"
      >
        <UContextMenu
          v-for="file in browser.filteredFiles.value"
          :key="file.name"
          :items="buildContextMenuItems(file)"
        >
          <div
            :data-testid="`file-entry-${file.name}`"
            :class="[
              'flex items-center gap-3 p-3 rounded-lg cursor-pointer transition-colors relative overflow-hidden',
              browser.isSelected(file)
                ? 'bg-primary/10'
                : 'hover:bg-muted/50',
              browser.isCutFile(file) && 'opacity-40',
            ]"
            @click="browser.onFileClick(file)"
          >
            <!-- Download progress background -->
            <div
              v-if="getFileTransferProgress(file) !== undefined"
              class="absolute inset-0 bg-primary/15 transition-all duration-300 ease-out"
              :style="{
                width: `${(getFileTransferProgress(file) ?? 0) * 100}%`,
              }"
            />
            <UCheckbox
              :model-value="browser.isSelected(file)"
              class="relative z-10"
              @click.stop
              @update:model-value="browser.toggleSelect(file)"
            />
            <!-- Thumbnail or icon -->
            <img
              v-if="browser.getThumbnailUrl(file)"
              :src="browser.getThumbnailUrl(file)!"
              :alt="file.name"
              class="w-8 h-8 rounded object-cover shrink-0 relative z-10"
              loading="lazy"
            >
            <UIcon
              v-else
              :name="
                file.isDir
                  ? 'i-lucide-folder'
                  : browser.getFileIcon(file.name)
              "
              :class="[
                'w-5 h-5 shrink-0 relative z-10',
                file.isDir ? 'text-primary' : 'text-muted',
              ]"
            />
            <div class="flex-1 min-w-0 relative z-10">
              <p class="text-sm truncate">{{ file.name }}</p>
              <div class="flex gap-3 text-xs text-muted mt-0.5">
                <span
                  v-if="file.displayPath"
                  class="text-primary/70"
                  >{{ file.displayPath }}/</span
                >
                <span v-if="file.modified">{{
                  browser.formatDate(file.modified)
                }}</span>
                <span v-if="!file.isDir && file.size">{{
                  browser.formatSize(file.size)
                }}</span>
              </div>
            </div>
            <UButton
              v-if="showPauseControl(file)"
              :icon="
                getFileTransferPaused(file) ? 'i-lucide-play' : 'i-lucide-pause'
              "
              color="neutral"
              variant="ghost"
              size="xs"
              class="relative z-10 shrink-0"
              :aria-label="
                getFileTransferPaused(file)
                  ? t('resumeTransfer')
                  : t('pauseTransfer')
              "
              @click.stop="togglePauseTransferAsync(file)"
            />
            <UButton
              v-if="getFileTransferProgress(file) !== undefined"
              icon="i-lucide-x"
              color="error"
              variant="ghost"
              size="xs"
              class="relative z-10 shrink-0"
              :aria-label="t('cancelTransfer')"
              @click.stop="cancelTransferAsync(file)"
            />
          </div>
        </UContextMenu>
      </div>

      <!-- ===== Grid view ===== -->
      <div
        v-else
        class="grid grid-cols-[repeat(auto-fill,minmax(140px,1fr))] gap-2"
      >
        <UContextMenu
          v-for="file in browser.filteredFiles.value"
          :key="file.name"
          :items="buildContextMenuItems(file)"
        >
          <div
            :data-testid="`file-entry-${file.name}`"
            :class="[
              'group relative flex flex-col items-center gap-2 p-3 rounded-lg cursor-pointer transition-colors overflow-hidden',
              browser.isSelected(file)
                ? 'bg-primary/10'
                : 'hover:bg-muted/50',
              browser.isCutFile(file) && 'opacity-40',
            ]"
            @click="browser.onFileClick(file)"
          >
            <!-- Selection checkbox (top-left, visible on hover or when selected) -->
            <UCheckbox
              :model-value="browser.isSelected(file)"
              :class="[
                'absolute top-2 left-2 z-10 transition-opacity',
                browser.isSelected(file)
                  ? 'opacity-100'
                  : 'opacity-0 group-hover:opacity-100',
              ]"
              @click.stop
              @update:model-value="browser.toggleSelect(file)"
            />
            <!-- Download progress background -->
            <div
              v-if="getFileTransferProgress(file) !== undefined"
              class="absolute inset-0 bg-primary/15 transition-all duration-300 ease-out"
              :style="{
                width: `${(getFileTransferProgress(file) ?? 0) * 100}%`,
              }"
            />
            <UButton
              v-if="showPauseControl(file)"
              :icon="
                getFileTransferPaused(file) ? 'i-lucide-play' : 'i-lucide-pause'
              "
              color="neutral"
              variant="solid"
              size="xs"
              class="absolute top-2 right-9 z-10"
              :aria-label="
                getFileTransferPaused(file)
                  ? t('resumeTransfer')
                  : t('pauseTransfer')
              "
              @click.stop="togglePauseTransferAsync(file)"
            />
            <UButton
              v-if="getFileTransferProgress(file) !== undefined"
              icon="i-lucide-x"
              color="error"
              variant="solid"
              size="xs"
              class="absolute top-2 right-2 z-10"
              :aria-label="t('cancelTransfer')"
              @click.stop="cancelTransferAsync(file)"
            />
            <!-- Thumbnail or icon -->
            <div
              class="w-full aspect-square rounded-md overflow-hidden flex items-center justify-center bg-muted/30"
            >
              <img
                v-if="browser.getThumbnailUrl(file)"
                :src="browser.getThumbnailUrl(file)!"
                :alt="file.name"
                class="w-full h-full object-cover"
                loading="lazy"
              >
              <UIcon
                v-else
                :name="
                  file.isDir
                    ? 'i-lucide-folder'
                    : browser.getFileIcon(file.name)
                "
                :class="[
                  'w-10 h-10',
                  file.isDir ? 'text-primary' : 'text-muted',
                ]"
              />
            </div>
            <!-- Filename + meta -->
            <div class="w-full min-w-0 text-center">
              <p class="text-xs truncate">{{ file.name }}</p>
              <p
                v-if="file.displayPath"
                class="text-[10px] text-primary/70 truncate mt-0.5"
              >
                {{ file.displayPath }}/
              </p>
              <p
                v-else-if="!file.isDir && file.size"
                class="text-[10px] text-muted mt-0.5"
              >
                {{ browser.formatSize(file.size) }}
              </p>
            </div>
          </div>
        </UContextMenu>
      </div>

      <!-- Searching indicator -->
      <div
        v-if="browser.isSearching.value"
        class="flex items-center justify-center gap-2 py-3 text-muted"
      >
        <UIcon
          name="i-lucide-loader-2"
          class="w-4 h-4 animate-spin"
        />
        <span class="text-xs">{{ t('searching') }}</span>
      </div>

      <!-- Loading more indicator -->
      <div
        v-if="browser.isLoadingMore.value"
        class="flex items-center justify-center gap-2 py-3 text-muted"
      >
        <UIcon
          name="i-lucide-loader-2"
          class="w-4 h-4 animate-spin"
        />
        <span class="text-xs"
          >{{
            browser.totalFiles.value - browser.filteredFiles.value.length
          }}
          {{ t('moreFiles') }}</span
        >
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { useFileBrowser } from '~/composables/useFileBrowser'

const props = defineProps<{
  browser: ReturnType<typeof useFileBrowser>
  peerStore: ReturnType<typeof usePeerStorageStore>
}>()

type RenameTarget = (typeof props.browser.filteredFiles.value)[number]

const emit = defineEmits<{
  openRenameDialog: [file: RenameTarget]
}>()

const { t } = useI18n()
const toast = useToast()

/**
 * Get transfer progress (0..1) for a file's row.
 *
 * Two sources, in priority order:
 *   1. S3 chunked download progress (`browser.getS3TransferProgress`) — keyed
 *      by the S3 object key, populated while `remote_storage_download_to_path`
 *      streams chunks for this file.
 *   2. P2P transfer progress (`peerStore.getTransferProgress`) — keyed by
 *      the full peer path, populated by the iroh streaming reader.
 *
 * Returns undefined when neither is active.
 */
const getFileTransferProgress = (file: { name: string; path?: string; isDir?: boolean }) => {
  if (!props.browser.selectedPeer.value) return undefined

  // S3 chunked download progress (composable handles the key derivation).
  if (props.browser.selectedPeer.value.s3BackendId) {
    const s3Progress = props.browser.getS3TransferProgress(file.name)
    if (s3Progress !== undefined) return s3Progress
  }

  const fullPath = (
    file.path || `${props.browser.currentPath.value}/${file.name}`
  ).replace(/\/+/g, '/')
  return props.peerStore.getTransferProgress(fullPath)
}

/**
 * Whether the P2P transfer for `file` is currently paused. Only P2P transfers
 * (iroh streaming) support pause; S3 and local shares always report false.
 */
const getFileTransferPaused = (file: { name: string; path?: string }) => {
  const peer = props.browser.selectedPeer.value
  if (!peer || peer.s3BackendId || peer.localPath) return false
  const fullPath = (
    file.path || `${props.browser.currentPath.value}/${file.name}`
  ).replace(/\/+/g, '/')
  return props.peerStore.getTransferPaused(fullPath)
}

/**
 * Show the pause/resume toggle only for an in-flight P2P transfer — S3 chunked
 * downloads have no pause control, and local shares never stream. A completed
 * transfer lingers in the store for ~1.5 s with `progress=1` so the bar can
 * animate the fill; the pause control must hide during that window.
 */
const showPauseControl = (file: { name: string; path?: string; isDir?: boolean }) => {
  const peer = props.browser.selectedPeer.value
  if (!peer || peer.s3BackendId || peer.localPath) return false
  const progress = getFileTransferProgress(file)
  return progress !== undefined && progress < 1
}

// --- Single-file actions invoked from the context menu ---
//
// Each helper wraps the corresponding `browser.*File()` call with toast
// reporting so the user gets feedback regardless of which menu item they
// invoked. Kept thin on purpose — the heavy lifting lives in the composable
// so the toolbar (which works on selections) and the context menu (which
// works on a single file) cannot drift apart.

const downloadFileAsync = async (file: RenameTarget) => {
  try {
    await props.browser.downloadFile(file)
  } catch (error) {
    toast.add({
      title: t('downloadFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}

const cancelTransferAsync = async (file: RenameTarget) => {
  try {
    await props.browser.cancelFileTransferAsync(file)
  } catch (error) {
    toast.add({
      title: t('cancelTransferFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}

const togglePauseTransferAsync = async (file: RenameTarget) => {
  try {
    await props.browser.togglePauseFileTransferAsync(file)
  } catch (error) {
    toast.add({
      title: t('pauseTransferFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}

const deleteFileAsync = async (file: RenameTarget) => {
  try {
    await props.browser.deleteFile(file)
    await props.browser.loadFiles()
  } catch (error) {
    toast.add({
      title: t('deleteFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}

const playFileAsync = async (file: RenameTarget) => {
  try {
    await props.browser.playFile(file)
  } catch (error) {
    toast.add({
      title: t('openFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}

/**
 * Build the items array for the per-row Nuxt UI `UContextMenu`. Grouped
 * (`ContextMenuItem[][]`) so the component renders separators between
 * groups:
 *
 *   1. open / download (file-only, omitted for folders)
 *   2. clipboard + rename
 *   3. delete
 *
 * Operations the current backend cannot perform are surfaced as
 * `disabled` rows so users see the feature exists but understand it
 * isn't available for the active peer/backend (e.g. delete on P2P
 * without a write UCAN).
 */
const buildContextMenuItems = (file: RenameTarget) => {
  const groups: Array<Array<Record<string, unknown>>> = []

  if (!file.isDir) {
    const fileActions: Array<Record<string, unknown>> = []
    if (props.browser.canPlayFile(file)) {
      fileActions.push({
        label: t('play'),
        icon: 'i-lucide-play',
        onSelect: () => playFileAsync(file),
      })
    }
    fileActions.push({
      label: t('download'),
      icon: 'i-lucide-download',
      onSelect: () => downloadFileAsync(file),
    })
    if (showPauseControl(file)) {
      fileActions.push({
        label: getFileTransferPaused(file) ? t('resumeTransfer') : t('pauseTransfer'),
        icon: getFileTransferPaused(file) ? 'i-lucide-play' : 'i-lucide-pause',
        onSelect: () => togglePauseTransferAsync(file),
      })
    }
    if (getFileTransferProgress(file) !== undefined) {
      fileActions.push({
        label: t('cancelTransfer'),
        icon: 'i-lucide-x',
        onSelect: () => cancelTransferAsync(file),
      })
    }
    groups.push(fileActions)
  }

  groups.push([
    {
      label: t('copy'),
      icon: 'i-lucide-copy',
      disabled: !props.browser.canCopyOrCutFile(file),
      onSelect: () => props.browser.copyFile(file),
    },
    {
      label: t('cut'),
      icon: 'i-lucide-scissors',
      disabled: !props.browser.canCopyOrCutFile(file),
      onSelect: () => props.browser.cutFile(file),
    },
    {
      label: t('rename'),
      icon: 'i-lucide-pencil',
      disabled: !props.browser.canRenameFile(file),
      onSelect: () => emit('openRenameDialog', file),
    },
  ])

  groups.push([
    {
      label: t('delete'),
      icon: 'i-lucide-trash-2',
      color: 'error',
      disabled: !props.browser.canDeleteFile(file),
      onSelect: () => deleteFileAsync(file),
    },
  ])

  return groups
}
</script>
