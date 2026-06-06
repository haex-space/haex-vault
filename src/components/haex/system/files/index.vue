<template>
  <HaexSystem>
    <!-- Header: Breadcrumbs + Actions -->
    <template #header>
      <div class="-my-1 space-y-2">
        <!-- Search + View toggle -->
        <div class="flex items-center gap-2">
          <UiInput
            v-model="browser.searchQuery.value"
            :placeholder="t('search')"
            class="flex-1"
            leading-icon="i-lucide-search"
            clearable
          />
          <div
            v-if="browser.selectedPeer.value"
            class="flex items-center rounded-lg border border-default"
          >
            <UiButton
              variant="ghost"
              icon="i-lucide-list"
              :color="browser.viewMode.value === 'list' ? 'primary' : 'neutral'"
              :title="t('viewList')"
              @click="browser.viewMode.value = 'list'"
            />
            <UiButton
              variant="ghost"
              icon="i-lucide-layout-grid"
              :color="browser.viewMode.value === 'grid' ? 'primary' : 'neutral'"
              :title="t('viewGrid')"
              @click="browser.viewMode.value = 'grid'"
            />
          </div>
        </div>

        <!-- Breadcrumbs + Actions -->
        <div class="flex items-center gap-2">
          <div class="flex items-center gap-1 flex-wrap flex-1 min-w-0">
            <UButton
              variant="ghost"
              color="neutral"
              icon="i-lucide-hard-drive"
              @click="browser.navigateToRoot()"
            >
              {{ t('title') }}
            </UButton>
            <template v-if="browser.selectedPeer.value">
              <UIcon
                name="i-lucide-chevron-right"
                class="w-3.5 h-3.5 text-muted shrink-0"
              />
              <UButton
                variant="ghost"
                color="neutral"
                :disabled="browser.currentPath.value === '/'"
                @click="browser.navigateToPath('/')"
              >
                {{ browser.selectedPeerName.value }}
              </UButton>
              <HaexPeerStatusDot
                v-if="!browser.selectedPeer.value?.s3BackendId"
                :status="ping.getStatus(browser.selectedPeer.value!.endpointId)"
                size="sm"
              />
              <template
                v-for="(segment, i) in browser.pathSegments.value"
                :key="i"
              >
                <UIcon
                  name="i-lucide-chevron-right"
                  class="w-3.5 h-3.5 text-muted shrink-0"
                />
                <UButton
                  variant="ghost"
                  color="neutral"
                  :disabled="i === browser.pathSegments.value.length - 1"
                  @click="browser.navigateToSegment(i)"
                >
                  {{ segment }}
                </UButton>
              </template>
            </template>
          </div>

          <!-- Selection actions -->
          <template v-if="browser.selectionCount.value > 0">
            <span class="text-xs font-medium text-primary shrink-0">
              {{ browser.selectionCount.value }} {{ t('selected') }}
            </span>
            <UiButton
              v-if="browser.selectedPeer.value?.localPath"
              variant="ghost"
              icon="i-lucide-copy"
              :title="t('copy')"
              @click="browser.copySelected()"
            />
            <UiButton
              v-if="browser.selectedPeer.value?.localPath"
              variant="ghost"
              icon="i-lucide-scissors"
              :title="t('cut')"
              @click="browser.cutSelected()"
            />
            <UiButton
              v-if="!browser.selectedPeer.value?.localPath"
              variant="ghost"
              icon="i-lucide-download"
              :title="t('download')"
              @click="browser.downloadSelectedAsync()"
            />
            <UiButton
              v-if="browser.selectedPeer.value?.localPath"
              variant="ghost"
              color="error"
              icon="i-lucide-trash-2"
              :title="t('delete')"
              @click="browser.deleteSelectedAsync()"
            />
            <UiButton
              variant="ghost"
              color="neutral"
              icon="i-lucide-x"
              @click="browser.clearSelection()"
            />
          </template>

          <!-- Paste button (no selection, clipboard has content) -->
          <UiButton
            v-else-if="browser.canPaste.value"
            variant="ghost"
            icon="i-lucide-clipboard-paste"
            @click="browser.pasteAsync()"
          >
            {{ t('paste') }} ({{ browser.clipboard.clipboardCount.value }})
          </UiButton>

          <!-- Upload + New Folder (when peer supports writes, no selection) -->
          <template
            v-if="
              browser.selectionCount.value === 0 &&
              !browser.canPaste.value &&
              browser.canWrite.value
            "
          >
            <UiButton
              variant="ghost"
              icon="i-lucide-folder-plus"
              :title="t('newFolder')"
              :loading="isCreatingFolder"
              @click="openCreateFolderDialog"
            />
            <UiButton
              variant="ghost"
              icon="i-lucide-upload"
              :title="t('uploadFiles')"
              :loading="isUploading"
              @click="uploadFilesAsync"
            />
          </template>

          <!-- P2P endpoint toggle + settings -->
          <template v-if="!browser.selectedPeer.value">
            <UiButton
              variant="ghost"
              icon="i-lucide-settings"
              :title="t('p2pSettings')"
              @click="openP2PSettings"
            />
            <UiButton
              :icon="
                peerStore.running ? 'i-lucide-power-off' : 'i-lucide-power'
              "
              :color="peerStore.running ? 'error' : 'primary'"
              :loading="isTogglingEndpoint"
              :title="
                peerStore.running ? t('stopEndpoint') : t('startEndpoint')
              "
              @click="toggleEndpointAsync"
            />
          </template>
        </div>
      </div>
    </template>

    <Transition
      :name="
        browser.direction.value === 'back' ? 'slide-back' : 'slide-forward'
      "
      mode="out-in"
    >
      <div
        :key="
          browser.selectedPeer.value
            ? `peer-${browser.currentPath.value}`
            : 'overview'
        "
        class="p-6 space-y-4"
      >
        <!-- File Browser (peer selected via deep-link or click) -->
        <div
          v-if="browser.selectedPeer.value"
          class="flex flex-col gap-4 h-full"
        >
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

        <!-- Storage overview (no peer selected) -->
        <div
          v-else
          class="flex flex-col gap-6 h-full"
        >
          <!-- Global search results -->
          <template v-if="browser.searchQuery.value">
            <!-- Searching, no results yet -->
            <div
              v-if="browser.isGlobalSearching.value && browser.filteredGlobalFiles.value.length === 0"
              class="flex items-center justify-center py-16 gap-2"
            >
              <UIcon
                name="i-lucide-loader-2"
                class="w-8 h-8 animate-spin text-muted"
              />
            </div>

            <!-- No results -->
            <div
              v-else-if="!browser.isGlobalSearching.value && browser.filteredGlobalFiles.value.length === 0"
              class="text-center py-16"
            >
              <UIcon
                name="i-lucide-search-x"
                class="w-12 h-12 mx-auto mb-2 opacity-30"
              />
              <p class="text-muted">{{ t('noResults') }}</p>
            </div>

            <!-- Results -->
            <div v-else class="space-y-1">
              <div
                v-for="file in browser.filteredGlobalFiles.value"
                :key="`${file.shareId}-${file.searchPath}`"
                class="flex items-center gap-3 p-3 rounded-lg cursor-pointer hover:bg-muted/50 transition-colors"
                @click="browser.onGlobalSearchResultClick(file)"
              >
                <UIcon
                  :name="file.isDir ? 'i-lucide-folder' : browser.getFileIcon(file.name)"
                  :class="['w-5 h-5 shrink-0', file.isDir ? 'text-primary' : 'text-muted']"
                />
                <div class="flex-1 min-w-0">
                  <p class="text-sm truncate">{{ file.name }}</p>
                  <div class="flex gap-3 text-xs text-muted mt-0.5">
                    <span class="text-primary/70">{{ file.displayPath }}/</span>
                    <span v-if="file.modified">{{ browser.formatDate(file.modified) }}</span>
                    <span v-if="!file.isDir && file.size">{{ browser.formatSize(file.size) }}</span>
                  </div>
                </div>
              </div>

              <!-- Still searching -->
              <div
                v-if="browser.isGlobalSearching.value"
                class="flex items-center justify-center gap-2 py-3 text-muted"
              >
                <UIcon
                  name="i-lucide-loader-2"
                  class="w-4 h-4 animate-spin"
                />
                <span class="text-xs">{{ t('searching') }}</span>
              </div>
            </div>
          </template>

          <!-- Normal overview (no search active) -->
          <template v-else>
            <!-- Grouping toggle -->
            <div
              v-if="hasAnyEntries"
              class="flex items-center justify-between"
            >
              <p class="text-xs font-medium text-muted uppercase tracking-wider">
                {{ t('groupBy.label') }}
              </p>
              <div class="flex items-center rounded-lg border border-default">
                <UiButton
                  variant="ghost"
                  icon="i-lucide-layers"
                  :color="groupBy === 'space' ? 'primary' : 'neutral'"
                  :title="t('groupBy.space')"
                  @click="groupBy = 'space'"
                >
                  {{ t('groupBy.space') }}
                </UiButton>
                <UiButton
                  variant="ghost"
                  icon="i-lucide-users"
                  :color="groupBy === 'contact' ? 'primary' : 'neutral'"
                  :title="t('groupBy.contact')"
                  @click="groupBy = 'contact'"
                >
                  {{ t('groupBy.contact') }}
                </UiButton>
              </div>
            </div>

            <!-- Grouped sections -->
            <div
              v-for="group in overviewGroups"
              :key="group.id"
            >
              <div class="flex items-center gap-2 mb-2">
                <UiAvatar
                  v-if="group.avatar"
                  :src="group.avatar.src"
                  :seed="group.avatar.seed"
                  :avatar-options="group.avatar.options"
                  :alt="group.avatar.alt"
                  size="xs"
                />
                <UIcon
                  v-else-if="group.icon"
                  :name="group.icon"
                  class="w-3.5 h-3.5 text-muted shrink-0"
                />
                <p
                  class="text-xs font-medium text-muted uppercase tracking-wider truncate"
                >
                  {{ group.title }}
                </p>
                <p
                  v-if="group.subtitle"
                  class="text-[10px] text-muted/70 truncate"
                >
                  {{ group.subtitle }}
                </p>
              </div>
              <div class="space-y-1">
                <div
                  v-for="entry in group.entries"
                  :key="entry.key"
                  :data-testid="`file-peer-${entry.peer.name}`"
                  class="flex items-center gap-3 p-3 rounded-lg bg-muted/30 hover:bg-muted/50 cursor-pointer transition-colors"
                  @click="browser.selectPeer(entry.peer)"
                >
                  <UiAvatar
                    v-if="entry.avatar"
                    :src="entry.avatar.src"
                    :seed="entry.avatar.seed"
                    :avatar-options="entry.avatar.options"
                    :alt="entry.avatar.alt"
                    :badge-src="entry.badge?.src"
                    :badge-seed="entry.badge?.seed"
                    :badge-alt="entry.badge?.alt"
                    size="sm"
                  />
                  <UIcon
                    v-else-if="entry.icon"
                    :name="entry.icon"
                    class="w-5 h-5 text-primary shrink-0"
                  />
                  <div class="flex-1 min-w-0">
                    <p class="text-sm font-medium truncate">{{ entry.title }}</p>
                    <p class="text-xs text-muted truncate">{{ entry.subtitle }}</p>
                  </div>
                  <HaexPeerStatusDot
                    v-if="entry.kind === 'remote-peer'"
                    :status="ping.getStatus(entry.peer.endpointId)"
                  />
                  <UIcon
                    name="i-lucide-chevron-right"
                    class="w-4 h-4 text-muted shrink-0"
                  />
                </div>
              </div>
            </div>

            <!-- Empty state -->
            <div
              v-if="!hasAnyEntries"
              class="flex flex-col items-center justify-center py-12 gap-3"
            >
              <UIcon
                name="i-lucide-hard-drive"
                class="w-12 h-12 opacity-30"
              />
              <p class="text-muted">{{ t('noStorage') }}</p>
              <p class="text-xs text-muted text-center">
                {{ t('noStorageHint') }}
              </p>
            </div>
          </template>
        </div>
      </div>
    </Transition>

    <!-- New folder dialog -->
    <UiDialogConfirm
      v-model:open="newFolderOpen"
      :title="t('newFolder')"
      :confirm-label="t('create')"
      @confirm="confirmCreateFolderAsync"
    >
      <template #body>
        <UiInput
          v-model="newFolderName"
          :placeholder="t('folderNamePlaceholder')"
          autofocus
          @keydown.enter="confirmCreateFolderAsync"
        />
      </template>
    </UiDialogConfirm>

    <!-- Rename dialog (triggered from per-row context menu) -->
    <UiDialogConfirm
      v-model:open="renameOpen"
      :title="t('renameTitle')"
      :confirm-label="t('rename')"
      :loading="isRenaming"
      @confirm="confirmRenameAsync"
    >
      <template #body>
        <UiInput
          v-model="renameNewName"
          :placeholder="t('renamePlaceholder')"
          autofocus
          @keydown.enter="confirmRenameAsync"
        />
      </template>
    </UiDialogConfirm>

    <!--
      Inline media preview for the active `browser.preview`. The URL may be
      a blob: (small local files), an asset: (regular local files), or a
      haex-stream: (S3 audio/video). The audio/video elements drive their
      own Range requests when the source supports them — we just hand over
      the URL.
    -->
    <UModal
      :open="browser.preview.isOpen.value"
      :title="browser.preview.previewFilename.value ?? ' '"
      :ui="{ content: 'max-w-3xl' }"
      @update:open="(v) => !v && browser.preview.close()"
    >
      <template #body>
        <div class="flex items-center justify-center min-h-32">
          <audio
            v-if="
              browser.preview.previewType.value === 'audio' &&
                browser.preview.previewUrl.value
            "
            data-testid="file-preview-audio"
            controls
            autoplay
            class="w-full"
            :src="browser.preview.previewUrl.value"
          />
          <video
            v-else-if="
              browser.preview.previewType.value === 'video' &&
                browser.preview.previewUrl.value
            "
            data-testid="file-preview-video"
            controls
            autoplay
            class="max-h-[70vh] w-full"
            :src="browser.preview.previewUrl.value"
          />
          <img
            v-else-if="
              browser.preview.previewType.value === 'image' &&
                browser.preview.previewUrl.value
            "
            :src="browser.preview.previewUrl.value"
            :alt="browser.preview.previewFilename.value ?? ''"
            class="max-h-[70vh] object-contain"
          >
          <iframe
            v-else-if="
              browser.preview.previewType.value === 'pdf' &&
                browser.preview.previewUrl.value
            "
            :src="browser.preview.previewUrl.value"
            class="w-full h-[70vh] border-0"
          />
        </div>
      </template>
    </UModal>
  </HaexSystem>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { SettingsCategory } from '~/config/settingsCategories'
import type { RemotePeer } from '~/composables/fileBrowserHelpers'
import { usePeerPing } from '~/composables/usePeerPing'
import type { StorageBackendInfo } from '~/../src-tauri/bindings/StorageBackendInfo'

const props = defineProps<{
  tabId: string
  windowParams?: Record<string, unknown>
}>()

const { t } = useI18n()
const peerStore = usePeerStorageStore()
const spacesStore = useSpacesStore()
const identityStore = useIdentityStore()

const browser = useFileBrowser(props.tabId)

type GroupBy = 'space' | 'contact'
const groupBy = ref<GroupBy>('space')

interface AvatarRef {
  src?: string | null
  seed?: string
  options?: Record<string, unknown> | null
  alt?: string
}

interface OverviewEntry {
  kind: 'local-share' | 'remote-peer' | 'cloud-backend'
  key: string
  title: string
  subtitle: string
  icon?: string
  avatar?: AvatarRef
  badge?: AvatarRef
  peer: RemotePeer
}

interface OverviewGroup {
  id: string
  title: string
  subtitle?: string
  icon?: string
  avatar?: AvatarRef
  entries: OverviewEntry[]
}

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
  if (!browser.selectedPeer.value) return undefined

  // S3 chunked download progress (composable handles the key derivation).
  if (browser.selectedPeer.value.s3BackendId) {
    const s3Progress = browser.getS3TransferProgress(file.name)
    if (s3Progress !== undefined) return s3Progress
  }

  const fullPath = (
    file.path || `${browser.currentPath.value}/${file.name}`
  ).replace(/\/+/g, '/')
  return peerStore.getTransferProgress(fullPath)
}

const isTogglingEndpoint = ref(false)
const toggleEndpointAsync = async () => {
  isTogglingEndpoint.value = true
  try {
    if (peerStore.running) await peerStore.stopAsync()
    else await peerStore.startAsync()
  } finally {
    isTogglingEndpoint.value = false
  }
}

// --- Upload + create folder ---
const toast = useToast()
const isUploading = ref(false)
const isCreatingFolder = ref(false)
const newFolderOpen = ref(false)
const newFolderName = ref('')

// --- Rename dialog (driven by the per-row context menu) ---
type RenameTarget = (typeof browser.filteredFiles.value)[number] | null
const renameOpen = ref(false)
const renameTarget = ref<RenameTarget>(null)
const renameNewName = ref('')
const isRenaming = ref(false)

const openRenameDialog = (file: NonNullable<RenameTarget>) => {
  renameTarget.value = file
  renameNewName.value = file.name
  renameOpen.value = true
}

const confirmRenameAsync = async () => {
  const target = renameTarget.value
  if (!target) return
  const next = renameNewName.value
  if (!next.trim() || next === target.name) {
    renameOpen.value = false
    return
  }
  isRenaming.value = true
  try {
    const ok = await browser.renameFile(target, next)
    if (ok) {
      renameOpen.value = false
      renameTarget.value = null
      renameNewName.value = ''
    } else {
      toast.add({ title: t('renameInvalid'), color: 'error' })
    }
  } catch (error) {
    toast.add({
      title: t('renameFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  } finally {
    isRenaming.value = false
  }
}

// --- Single-file actions invoked from the context menu ---
//
// Each helper wraps the corresponding `browser.*File()` call with toast
// reporting so the user gets feedback regardless of which menu item they
// invoked. Kept thin on purpose — the heavy lifting lives in the composable
// so the toolbar (which works on selections) and the context menu (which
// works on a single file) cannot drift apart.

const downloadFileAsync = async (file: NonNullable<RenameTarget>) => {
  try {
    await browser.downloadFile(file)
  } catch (error) {
    toast.add({
      title: t('downloadFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}

const cancelTransferAsync = async (file: NonNullable<RenameTarget>) => {
  try {
    await browser.cancelFileTransferAsync(file)
  } catch (error) {
    toast.add({
      title: t('cancelTransferFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}

const deleteFileAsync = async (file: NonNullable<RenameTarget>) => {
  try {
    await browser.deleteFile(file)
    await browser.loadFiles()
  } catch (error) {
    toast.add({
      title: t('deleteFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}

const playFileAsync = async (file: NonNullable<RenameTarget>) => {
  try {
    await browser.playFile(file)
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
const buildContextMenuItems = (file: NonNullable<RenameTarget>) => {
  const groups: Array<Array<Record<string, unknown>>> = []

  if (!file.isDir) {
    const fileActions: Array<Record<string, unknown>> = []
    if (browser.canPlayFile(file)) {
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
      disabled: !browser.canCopyOrCutFile(file),
      onSelect: () => browser.copyFile(file),
    },
    {
      label: t('cut'),
      icon: 'i-lucide-scissors',
      disabled: !browser.canCopyOrCutFile(file),
      onSelect: () => browser.cutFile(file),
    },
    {
      label: t('rename'),
      icon: 'i-lucide-pencil',
      disabled: !browser.canRenameFile(file),
      onSelect: () => openRenameDialog(file),
    },
  ])

  groups.push([
    {
      label: t('delete'),
      icon: 'i-lucide-trash-2',
      color: 'error',
      disabled: !browser.canDeleteFile(file),
      onSelect: () => deleteFileAsync(file),
    },
  ])

  return groups
}

const uploadFilesAsync = async () => {
  isUploading.value = true
  try {
    const count = await browser.uploadFilesAsync()
    if (count > 0) {
      toast.add({
        title: t('uploadSuccess', { count }),
        color: 'success',
      })
    }
  } catch (error) {
    toast.add({
      title: t('uploadFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  } finally {
    isUploading.value = false
  }
}

const openCreateFolderDialog = () => {
  newFolderName.value = ''
  newFolderOpen.value = true
}

const confirmCreateFolderAsync = async () => {
  const name = newFolderName.value
  if (!name.trim()) return
  isCreatingFolder.value = true
  try {
    const ok = await browser.createFolderAsync(name)
    if (ok) {
      newFolderOpen.value = false
      newFolderName.value = ''
    } else {
      toast.add({ title: t('folderNameInvalid'), color: 'error' })
    }
  } catch (error) {
    toast.add({
      title: t('createFolderFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  } finally {
    isCreatingFolder.value = false
  }
}

/**
 * Identifies whether a given endpoint id belongs to this device. Returns
 * true when `peerStore.nodeId` is empty so we never expose an own
 * `haex_space_devices` row as a remote peer during the brief window
 * before `refreshStatusAsync` resolves (or after `stopAsync`, which
 * resets `nodeId` to '' even though the row in DB is still ours).
 *
 * Biases toward "treat unknown endpoints as own" because the alternative
 * — surfacing the local device as a peer — is the more confusing failure
 * mode. Once `nodeId` is populated this collapses to the strict equality
 * check.
 */
const isOwnEndpoint = (endpointId: string): boolean => {
  if (!peerStore.nodeId) return true
  return endpointId === peerStore.nodeId
}

// Aggregate remote peers from spaces + contacts
const contactClaims = ref<Record<string, { type: string; value: string }[]>>({})
const loadContactClaimsAsync = async () => {
  for (const contact of identityStore.contacts) {
    const claims = await identityStore.getClaimsAsync(contact.id)
    contactClaims.value[contact.id] = claims.map((c) => ({
      type: c.type,
      value: c.value,
    }))
  }
}

// Own device shares (browsable locally without P2P)
const windowManager = useWindowManagerStore()
const openP2PSettings = () => {
  windowManager.openWindowAsync({
    type: 'system',
    sourceId: 'settings',
    params: { category: SettingsCategory.Spaces },
  })
}

// When endpoint is running, filter by nodeId. Otherwise show all shares
// (they were all registered by this device since they have local paths).
const localShares = computed(() => {
  if (peerStore.nodeId) {
    return peerStore.shares.filter(
      (s) => s.endpointId === peerStore.nodeId,
    )
  }
  return peerStore.shares
})

const getSpaceName = (spaceId: string) => {
  return (
    spacesStore.visibleSpaces.find((s) => s.id === spaceId)?.name ||
    spaceId.slice(0, 8)
  )
}

const remotePeers = computed(() => {
  const peers: RemotePeer[] = []
  const seen = new Set<string>()

  for (const device of peerStore.spaceDevices) {
    if (isOwnEndpoint(device.endpointId)) continue
    if (seen.has(device.endpointId)) continue
    seen.add(device.endpointId)
    peers.push({
      endpointId: device.endpointId,
      name: device.name || device.endpointId.slice(0, 16) + '...',
      source: 'space',
      detail: getSpaceName(device.spaceId),
    })
  }

  for (const contact of identityStore.contacts) {
    const claims = contactClaims.value[contact.id] || []
    for (const claim of claims) {
      if (!claim.type.startsWith('device:') || !claim.value) continue
      if (seen.has(claim.value)) continue
      seen.add(claim.value)
      peers.push({
        endpointId: claim.value,
        name: `${contact.name} (${claim.type.replace('device:', '')})`,
        source: 'contact',
        detail: contact.name,
      })
    }
  }

  return peers
})

const remotePeerIds = computed(() => remotePeers.value.map((p) => p.endpointId))
const ping = usePeerPing(remotePeerIds)

const parseAvatarOptions = (raw: string | null | undefined) => {
  if (!raw) return null
  try {
    return JSON.parse(raw) as Record<string, unknown>
  } catch {
    return null
  }
}

const getIdentity = (identityId: string | null | undefined) => {
  if (!identityId) return undefined
  return identityStore.identities.find((i) => i.id === identityId)
}

const identityAvatarFromIdentity = (
  identity: ReturnType<typeof getIdentity>,
): AvatarRef | undefined => {
  if (!identity) return undefined
  return {
    src: identity.avatar,
    seed: identity.id,
    options: parseAvatarOptions(identity.avatarOptions),
    alt: identity.name,
  }
}

const localShareEntry = (
  share: typeof localShares.value[number],
): OverviewEntry => ({
  kind: 'local-share',
  key: `local:${share.id}`,
  title: share.name,
  subtitle: t('sections.thisDevice'),
  icon: 'i-lucide-folder',
  peer: {
    endpointId: peerStore.nodeId,
    name: share.name,
    source: 'space',
    detail: t('sections.thisDevice'),
    localPath: share.localPath,
  },
})

interface PeerEntryInput {
  endpointId: string
  contextKey: string
  detail: string
  source: RemotePeer['source']
  // Optional rich data (preferred when available)
  device?: typeof peerStore.spaceDevices[number]
  identityId?: string | null
  // Fallback name when neither identity nor device row are available
  fallbackName?: string
}

const buildPeerEntry = (input: PeerEntryInput): OverviewEntry => {
  const identity = getIdentity(input.identityId ?? input.device?.identityId)
  const contactName = identity?.name?.trim() || undefined
  const deviceName =
    input.device?.name?.trim() ||
    input.fallbackName?.trim() ||
    `${input.endpointId.slice(0, 16)}…`

  // Title prefers the contact's known identity name. Subtitle keeps the
  // device name visible when it differs, plus the existing detail line
  // (typically the space name).
  const title = contactName || deviceName
  const showDeviceInSubtitle =
    !!contactName && contactName.toLowerCase() !== deviceName.toLowerCase()
  const subtitle = showDeviceInSubtitle
    ? `${deviceName} · ${input.detail}`
    : input.detail

  const avatar: AvatarRef | undefined = input.device
    ? {
        src: input.device.avatar,
        seed: input.device.endpointId,
        options: parseAvatarOptions(input.device.avatarOptions),
        alt: deviceName,
      }
    : identity
      ? identityAvatarFromIdentity(identity)
      : { seed: input.endpointId, alt: deviceName }

  // Badge is the contact's identity avatar — only shown when we actually
  // have a known identity to badge with AND we already render a separate
  // device avatar (otherwise the identity avatar is the main avatar).
  const badge: AvatarRef | undefined =
    input.device && identity ? identityAvatarFromIdentity(identity) : undefined

  return {
    kind: 'remote-peer',
    key: `remote:${input.contextKey}:${input.endpointId}`,
    title,
    subtitle,
    icon: input.source === 'contact' ? 'i-lucide-user' : 'i-lucide-monitor',
    avatar,
    badge,
    peer: {
      endpointId: input.endpointId,
      name: title,
      source: input.source,
      detail: input.detail,
    },
  }
}

// S3 / remote storage backends. These live outside the space + contact model
// (they belong to no peer), so they get their own group that is always
// appended last regardless of the current grouping mode.
const storageBackends = ref<StorageBackendInfo[]>([])
const loadStorageBackendsAsync = async () => {
  try {
    storageBackends.value = await invoke<StorageBackendInfo[]>(
      'remote_storage_list_backends',
    )
  } catch {
    // Non-fatal: the file browser must still render the other sections even
    // if S3 listing fails (e.g. database error). The settings page is the
    // canonical place to diagnose backend configuration issues.
    storageBackends.value = []
  }
}

const s3PeerForBackend = (backend: StorageBackendInfo): RemotePeer => ({
  endpointId: `s3:${backend.id}`,
  name: backend.name,
  source: 's3',
  detail: backend.config?.bucket || backend.type,
  s3BackendId: backend.id,
})

const cloudStorageGroup = computed<OverviewGroup | null>(() => {
  const enabled = storageBackends.value.filter((b) => b.enabled)
  if (enabled.length === 0) return null
  return {
    id: 'cloud-storage',
    title: t('groups.cloudStorage'),
    icon: 'i-lucide-cloud',
    entries: enabled.map((backend) => ({
      kind: 'cloud-backend' as const,
      key: `s3:${backend.id}`,
      title: backend.name,
      subtitle: backend.config?.bucket
        ? `${backend.type.toUpperCase()} · ${backend.config.bucket}`
        : backend.type.toUpperCase(),
      icon: 'i-lucide-cloud',
      peer: s3PeerForBackend(backend),
    })),
  }
})

const overviewGroups = computed<OverviewGroup[]>(() => {
  const groups =
    groupBy.value === 'space' ? buildSpaceGroups() : buildContactGroups()
  const cloud = cloudStorageGroup.value
  return cloud ? [...groups, cloud] : groups
})

const hasAnyEntries = computed(() => overviewGroups.value.length > 0)

// Phantom-row guard: `peerStore.spaceDevices` mirrors every haex_space_devices
// row in the local DB, including ones that arrived via CRDT sync of a space
// the user never joined. The spaces store already filters those at the
// `visibleSpaces` boundary (membership cross-check + owner fallback), so any
// device whose spaceId is outside that set must not surface in the UI.
const visibleSpaceIds = computed(
  () => new Set(spacesStore.visibleSpaces.map((s) => s.id)),
)
const isDeviceInVisibleSpace = (spaceId: string): boolean =>
  visibleSpaceIds.value.has(spaceId)

function buildSpaceGroups(): OverviewGroup[] {
  // Bucket entries strictly by spaceId. Two spaces with the same name but
  // different ids stay as two separate groups — they are different spaces
  // by identity and must not be merged. The shortened spaceId is shown as
  // subtitle so the user can tell them apart.
  const buckets = new Map<string, OverviewEntry[]>()
  const seenDevicesPerSpace = new Map<string, Set<string>>()
  const seenSharesPerSpace = new Map<string, Set<string>>()

  const pushEntry = (spaceId: string, entry: OverviewEntry) => {
    const list = buckets.get(spaceId)
    if (list) list.push(entry)
    else buckets.set(spaceId, [entry])
  }

  for (const share of localShares.value) {
    let seen = seenSharesPerSpace.get(share.spaceId)
    if (!seen) {
      seen = new Set()
      seenSharesPerSpace.set(share.spaceId, seen)
    }
    if (seen.has(share.id)) continue
    seen.add(share.id)
    pushEntry(share.spaceId, localShareEntry(share))
  }

  for (const device of peerStore.spaceDevices) {
    if (isOwnEndpoint(device.endpointId)) continue
    if (!isDeviceInVisibleSpace(device.spaceId)) continue
    let seen = seenDevicesPerSpace.get(device.spaceId)
    if (!seen) {
      seen = new Set()
      seenDevicesPerSpace.set(device.spaceId, seen)
    }
    if (seen.has(device.endpointId)) continue
    seen.add(device.endpointId)
    pushEntry(
      device.spaceId,
      buildPeerEntry({
        endpointId: device.endpointId,
        contextKey: `space:${device.spaceId}`,
        detail: getSpaceName(device.spaceId),
        source: 'space',
        device,
      }),
    )
  }

  const groups: OverviewGroup[] = []
  const consumedSpaceIds = new Set<string>()

  const groupForSpace = (
    spaceId: string,
    title: string,
    ownerIdentityId?: string | null,
  ): OverviewGroup => {
    const ownerIdentity = getIdentity(ownerIdentityId)
    return {
      id: `space:${spaceId}`,
      title,
      subtitle: shortSpaceId(spaceId),
      icon: 'i-lucide-layers',
      avatar: identityAvatarFromIdentity(ownerIdentity),
      entries: buckets.get(spaceId) ?? [],
    }
  }

  for (const space of spacesStore.visibleSpaces) {
    if (consumedSpaceIds.has(space.id)) continue
    consumedSpaceIds.add(space.id)
    const entries = buckets.get(space.id)
    if (!entries || entries.length === 0) continue
    groups.push(groupForSpace(space.id, space.name, space.ownerIdentityId))
  }

  // No orphan-spaceId fallback by design: if a bucket's spaceId is not in
  // `visibleSpaces`, the user is not a member and we must not surface that
  // space in the UI — the phantom row got dropped at the bucket-fill step.

  // Direct contact devices (claim-only, not in any space).
  // `peerStore.spaceDevices` is pre-filtered to visible spaces above, so
  // this Set already excludes phantom rows that could otherwise shadow a
  // contact claim sharing the same endpoint.
  const knownEndpointIds = new Set(
    peerStore.spaceDevices
      .filter((d) => isDeviceInVisibleSpace(d.spaceId))
      .map((d) => d.endpointId),
  )
  const directEntries: OverviewEntry[] = []
  const seen = new Set<string>()
  for (const contact of identityStore.contacts) {
    const claims = contactClaims.value[contact.id] || []
    for (const claim of claims) {
      if (!claim.type.startsWith('device:') || !claim.value) continue
      if (knownEndpointIds.has(claim.value)) continue
      if (seen.has(claim.value)) continue
      seen.add(claim.value)
      directEntries.push(
        buildPeerEntry({
          endpointId: claim.value,
          contextKey: 'direct-contacts',
          detail: contact.name,
          source: 'contact',
          identityId: contact.id,
          fallbackName: claim.type.replace('device:', ''),
        }),
      )
    }
  }
  if (directEntries.length > 0) {
    groups.push({
      id: 'direct-contacts',
      title: t('groups.directContacts'),
      icon: 'i-lucide-user',
      entries: directEntries,
    })
  }

  return groups
}

function buildContactGroups(): OverviewGroup[] {
  const groups: OverviewGroup[] = []
  const ownIdentityIds = new Set(
    identityStore.ownIdentities.map((i) => i.id),
  )

  // "My devices" — local shares + space devices linked to own identities
  const myEntries: OverviewEntry[] = []
  for (const share of localShares.value) {
    myEntries.push(localShareEntry(share))
  }
  const seenForMe = new Set<string>()
  for (const device of peerStore.spaceDevices) {
    if (isOwnEndpoint(device.endpointId)) continue
    if (seenForMe.has(device.endpointId)) continue
    if (!device.identityId || !ownIdentityIds.has(device.identityId)) continue
    if (!isDeviceInVisibleSpace(device.spaceId)) continue
    seenForMe.add(device.endpointId)
    myEntries.push(
      buildPeerEntry({
        endpointId: device.endpointId,
        contextKey: 'me',
        detail: getSpaceName(device.spaceId),
        source: 'space',
        device,
      }),
    )
  }
  if (myEntries.length > 0) {
    const ownIdentity = identityStore.ownIdentities[0]
    groups.push({
      id: 'me',
      title: t('groups.myDevices'),
      subtitle: ownIdentity?.did ? shortDid(ownIdentity.did) : undefined,
      icon: 'i-lucide-user-check',
      avatar: identityAvatarFromIdentity(ownIdentity),
      entries: myEntries,
    })
  }

  // One group per contact
  for (const contact of identityStore.contacts) {
    const entries: OverviewEntry[] = []
    const seen = new Set<string>()

    for (const device of peerStore.spaceDevices) {
      if (isOwnEndpoint(device.endpointId)) continue
      if (device.identityId !== contact.id) continue
      if (!isDeviceInVisibleSpace(device.spaceId)) continue
      if (seen.has(device.endpointId)) continue
      seen.add(device.endpointId)
      entries.push(
        buildPeerEntry({
          endpointId: device.endpointId,
          contextKey: `contact:${contact.id}`,
          detail: getSpaceName(device.spaceId),
          source: 'space',
          device,
        }),
      )
    }

    const claims = contactClaims.value[contact.id] || []
    for (const claim of claims) {
      if (!claim.type.startsWith('device:') || !claim.value) continue
      if (seen.has(claim.value)) continue
      seen.add(claim.value)
      entries.push(
        buildPeerEntry({
          endpointId: claim.value,
          contextKey: `contact:${contact.id}`,
          detail: contact.name,
          source: 'contact',
          identityId: contact.id,
          fallbackName: claim.type.replace('device:', ''),
        }),
      )
    }

    if (entries.length > 0) {
      groups.push({
        id: `contact:${contact.id}`,
        title: contact.name,
        subtitle: shortDid(contact.did),
        icon: 'i-lucide-user',
        avatar: identityAvatarFromIdentity(contact),
        entries,
      })
    }
  }

  // Devices we know about but cannot attribute to any identity
  const attributedEndpoints = new Set<string>()
  for (const g of groups) {
    for (const e of g.entries) attributedEndpoints.add(e.peer.endpointId)
  }
  const unattributed: OverviewEntry[] = []
  const seenUnattr = new Set<string>()
  for (const device of peerStore.spaceDevices) {
    if (isOwnEndpoint(device.endpointId)) continue
    if (attributedEndpoints.has(device.endpointId)) continue
    if (!isDeviceInVisibleSpace(device.spaceId)) continue
    if (seenUnattr.has(device.endpointId)) continue
    seenUnattr.add(device.endpointId)
    unattributed.push(
      buildPeerEntry({
        endpointId: device.endpointId,
        contextKey: 'unknown',
        detail: getSpaceName(device.spaceId),
        source: 'space',
        device,
      }),
    )
  }
  if (unattributed.length > 0) {
    groups.push({
      id: 'unknown',
      title: t('groups.unknown'),
      icon: 'i-lucide-help-circle',
      entries: unattributed,
    })
  }

  return groups
}

function shortDid(did: string): string {
  if (did.length <= 24) return did
  return `${did.slice(0, 16)}…${did.slice(-6)}`
}

function shortSpaceId(id: string): string {
  if (id.length <= 12) return id
  return `${id.slice(0, 8)}…${id.slice(-4)}`
}

const applyDeepLink = async (params?: Record<string, unknown>) => {
  if (!params?.endpointId) return

  const endpointId = params.endpointId as string
  const peerName =
    (params.peerName as string) || endpointId.slice(0, 16) + '...'
  const localPath = params.localPath as string | undefined
  const shareName = params.shareName as string | undefined

  const existing = remotePeers.value.find((p) => p.endpointId === endpointId)
  const peer = existing || {
    endpointId,
    name: peerName,
    source: 'space' as const,
    detail: shareName || '',
    localPath,
  }
  if (existing && localPath && !existing.localPath) {
    peer.localPath = localPath
  }
  browser.setInitialPeer(peer)
  await browser.loadFiles()
}

// React to param changes (singleton window gets params merged on re-open)
watch(
  () => props.windowParams,
  (params) => {
    if (params?.endpointId) applyDeepLink(params)
  },
  { deep: true },
)

onMounted(async () => {
  // Load identities first so `spacesStore.visibleSpaces` can resolve owner
  // and membership filters against the user's own identities — without it,
  // the membership cross-check inside the spaces store would run against an
  // empty ownIdentities set and hide every legitimate space until the next
  // reload.
  await identityStore.loadIdentitiesAsync()
  await Promise.all([
    peerStore.refreshStatusAsync(),
    peerStore.loadSharesAsync(),
    peerStore.loadSpaceDevicesAsync(),
    spacesStore.loadSpacesFromDbAsync(),
    loadStorageBackendsAsync(),
  ])
  await loadContactClaimsAsync()
  await applyDeepLink(props.windowParams)
})
</script>

<i18n lang="yaml">
de:
  title: Dateien
  description: Dateien von verbundenen Geräten durchsuchen und herunterladen
  devices: Geräte
  endpointStopped: P2P-Endpoint ist nicht gestartet
  startEndpoint: Endpoint starten
  stopEndpoint: Endpoint stoppen
  emptyFolder: Ordner ist leer
  noResults: Keine Treffer
  searching: Verzeichnisse werden durchsucht…
  retry: Erneut versuchen
  downloaded: '"{name}" heruntergeladen'
  downloadFailed: Download fehlgeschlagen

  search: Suchen…
  viewList: Listenansicht
  viewGrid: Kachelansicht
  download: Herunterladen
  moreFiles: weitere Dateien werden geladen…
  selected: ausgewählt
  selectAll: Alle auswählen
  copy: Kopieren
  cut: Ausschneiden
  paste: Einfügen
  delete: Löschen
  cancel: Abbrechen
  p2pSettings: P2P-Einstellungen
  noStorage: Keine Speicherquellen verfügbar
  noStorageHint: Teile Ordner in den P2P-Einstellungen oder verbinde dich mit anderen Geräten.
  uploadFiles: Dateien hochladen
  uploadSuccess: '{count} Datei(en) hinzugefügt'
  uploadFailed: Upload fehlgeschlagen
  newFolder: Neuer Ordner
  folderNamePlaceholder: Ordnername
  folderNameInvalid: Ungültiger Ordnername
  createFolderFailed: Ordner konnte nicht erstellt werden
  create: Erstellen
  play: Abspielen
  rename: Umbenennen
  renameTitle: Datei umbenennen
  renamePlaceholder: Neuer Name
  renameInvalid: Ungültiger Name
  renameFailed: Umbenennen fehlgeschlagen
  deleteFailed: Löschen fehlgeschlagen
  openFailed: Öffnen fehlgeschlagen
  cancelTransfer: Übertragung abbrechen
  cancelTransferFailed: Übertragung konnte nicht abgebrochen werden
  sections:
    local: Dieses Gerät
    peers: Andere Geräte
    thisDevice: Lokaler Ordner
  groupBy:
    label: Gruppierung
    space: Nach Space
    contact: Nach Kontakt
  groups:
    myDevices: Meine Geräte
    directContacts: Direkte Kontakte
    unknown: Ohne Zuordnung
    cloudStorage: Cloud-Speicher
en:
  title: Files
  description: Browse and download files from connected devices
  devices: Devices
  endpointStopped: P2P endpoint is not running
  startEndpoint: Start endpoint
  stopEndpoint: Stop endpoint
  emptyFolder: Folder is empty
  noResults: No matches
  searching: Searching directories…
  retry: Retry
  downloaded: '"{name}" downloaded'
  downloadFailed: Download failed

  search: Search…
  viewList: List view
  viewGrid: Grid view
  download: Download
  moreFiles: more files loading…
  selected: selected
  selectAll: Select all
  copy: Copy
  cut: Cut
  paste: Paste
  delete: Delete
  cancel: Cancel
  p2pSettings: P2P Settings
  noStorage: No storage sources available
  noStorageHint: Share folders in P2P settings or connect with other devices.
  uploadFiles: Upload files
  uploadSuccess: '{count} file(s) added'
  uploadFailed: Upload failed
  newFolder: New folder
  folderNamePlaceholder: Folder name
  folderNameInvalid: Invalid folder name
  createFolderFailed: Could not create folder
  create: Create
  play: Play
  rename: Rename
  renameTitle: Rename file
  renamePlaceholder: New name
  renameInvalid: Invalid name
  renameFailed: Rename failed
  deleteFailed: Delete failed
  openFailed: Open failed
  cancelTransfer: Cancel transfer
  cancelTransferFailed: Could not cancel transfer
  sections:
    local: This device
    peers: Other devices
    thisDevice: Local folder
  groupBy:
    label: Group by
    space: By space
    contact: By contact
  groups:
    myDevices: My devices
    directContacts: Direct contacts
    unknown: Unattributed
    cloudStorage: Cloud storage
</i18n>
