<template>
  <div class="-my-1 space-y-2">
    <!-- Search + View toggle -->
    <div class="flex items-center gap-2">
      <UiInput
        v-model="searchQuery"
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
          :color="viewMode === 'list' ? 'primary' : 'neutral'"
          :title="t('viewList')"
          @click="viewMode = 'list'"
        />
        <UiButton
          variant="ghost"
          icon="i-lucide-layout-grid"
          :color="viewMode === 'grid' ? 'primary' : 'neutral'"
          :title="t('viewGrid')"
          @click="viewMode = 'grid'"
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
            :path-type="connectionType.getPathType(browser.selectedPeer.value!.endpointId)"
            :rtt-ms="connectionType.getRttMs(browser.selectedPeer.value!.endpointId)"
            size="sm"
            @hover="emit('refreshPeerStatus', browser.selectedPeer.value!.endpointId)"
          />
          <span
            v-if="!browser.selectedPeer.value?.s3BackendId && aggregateBytesPerSec > 0"
            class="inline-flex items-center gap-1 text-xs font-mono text-muted tabular-nums"
            :title="t('downloadThroughputTooltip')"
          >
            <UIcon name="i-lucide-arrow-down" class="w-3 h-3" />
            {{ formatBytesPerSec(aggregateBytesPerSec) }}
          </span>
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
          @click="emit('openCreateFolderDialog')"
        />
        <UiButton
          variant="ghost"
          icon="i-lucide-upload"
          :title="t('uploadFiles')"
          :loading="isUploading"
          @click="emit('uploadFiles')"
        />
      </template>

      <!-- P2P endpoint toggle + settings -->
      <template v-if="!browser.selectedPeer.value">
        <UiButton
          variant="ghost"
          icon="i-lucide-settings"
          :title="t('p2pSettings')"
          @click="emit('openP2pSettings')"
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
          @click="emit('toggleEndpoint')"
        />
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { useFileBrowser } from '~/composables/useFileBrowser'
import type { usePeerPing } from '~/composables/usePeerPing'
import type { usePeerConnectionType } from '~/composables/usePeerConnectionType'

defineProps<{
  browser: ReturnType<typeof useFileBrowser>
  peerStore: ReturnType<typeof usePeerStorageStore>
  ping: ReturnType<typeof usePeerPing>
  connectionType: ReturnType<typeof usePeerConnectionType>
  aggregateBytesPerSec: number
  isUploading: boolean
  isCreatingFolder: boolean
  isTogglingEndpoint: boolean
}>()

const emit = defineEmits<{
  refreshPeerStatus: [endpointId: string]
  toggleEndpoint: []
  openCreateFolderDialog: []
  uploadFiles: []
  openP2pSettings: []
}>()

// Two-way bindings for parent-owned ref state — avoids `vue/no-mutating-props`
// while keeping the parent's `useFileBrowser` refs as the source of truth.
const searchQuery = defineModel<string>('searchQuery', { required: true })
const viewMode = defineModel<'list' | 'grid'>('viewMode', { required: true })

const { t } = useI18n()

const formatBytesPerSec = (bps: number): string => {
  if (bps < 1024) return `${bps.toFixed(0)} B/s`
  if (bps < 1024 * 1024) return `${(bps / 1024).toFixed(1)} KB/s`
  if (bps < 1024 * 1024 * 1024) return `${(bps / 1024 / 1024).toFixed(1)} MB/s`
  return `${(bps / 1024 / 1024 / 1024).toFixed(2)} GB/s`
}
</script>
