<template>
  <div class="flex flex-col gap-6 h-full">
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
            @click="emit('update:groupBy', 'space')"
          >
            {{ t('groupBy.space') }}
          </UiButton>
          <UiButton
            variant="ghost"
            icon="i-lucide-users"
            :color="groupBy === 'contact' ? 'primary' : 'neutral'"
            :title="t('groupBy.contact')"
            @click="emit('update:groupBy', 'contact')"
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
              :path-type="connectionType.getPathType(entry.peer.endpointId)"
              :rtt-ms="connectionType.getRttMs(entry.peer.endpointId)"
              @hover="emit('refreshPeerStatus', entry.peer.endpointId)"
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
</template>

<script setup lang="ts">
import type { useFileBrowser } from '~/composables/useFileBrowser'
import type { usePeerPing } from '~/composables/usePeerPing'
import type { usePeerConnectionType } from '~/composables/usePeerConnectionType'
import type {
  GroupBy,
  OverviewGroup,
} from '~/composables/useFilesOverviewGroups'

defineProps<{
  browser: ReturnType<typeof useFileBrowser>
  overviewGroups: OverviewGroup[]
  hasAnyEntries: boolean
  groupBy: GroupBy
  ping: ReturnType<typeof usePeerPing>
  connectionType: ReturnType<typeof usePeerConnectionType>
}>()

const emit = defineEmits<{
  'update:groupBy': [GroupBy]
  refreshPeerStatus: [endpointId: string]
}>()

const { t } = useI18n()
</script>
