<template>
  <UContextMenu :items="workspaceStore.getWorkspaceContextMenuItems(workspace.id)">
    <HaexDesktopWorkspaceDropZone
      :workspace-id="workspace.id"
      :background-style="workspaceStore.getWorkspaceBackgroundStyle(workspace)"
      @desktop-click="emit('desktop-click')"
      @area-select-start="(e) => emit('area-select-start', e)"
      @drag-over="(e) => emit('drag-over', e)"
      @drop="(e) => emit('drop', e, workspace.id)"
    >
      <!-- Drop Target Zone (visible during drag) -->
      <div
        v-if="dropTargetZone"
        class="absolute border-2 border-blue-500 bg-blue-500/10 rounded-lg pointer-events-none z-10 transition-all duration-75"
        :style="{
          left: `${dropTargetZone.x}px`,
          top: `${dropTargetZone.y}px`,
          width: `${dropTargetZone.width}px`,
          height: `${dropTargetZone.height}px`,
        }"
      />

      <!-- Snap Dropzones (only visible when window drag near edge) -->

      <div
        class="absolute left-0 top-0 bottom-0 border-blue-500 pointer-events-none backdrop-blur-sm z-50 transition-all duration-500 ease-in-out"
        :class="
          showLeftSnapZone ? 'w-1/2 bg-blue-500/20 border-2' : 'w-0'
        "
      />

      <div
        class="absolute right-0 top-0 bottom-0 border-blue-500 pointer-events-none backdrop-blur-sm z-50 transition-all duration-500 ease-in-out"
        :class="
          showRightSnapZone ? 'w-1/2 bg-blue-500/20 border-2' : 'w-0'
        "
      />

      <!-- Area Selection Box -->
      <div
        v-if="isAreaSelecting"
        class="absolute bg-blue-500/20 border-2 border-blue-500 pointer-events-none z-30"
        :style="selectionBoxStyle"
      />

      <!-- Icons for this workspace -->
      <HaexDesktopIcon
        v-for="item in desktopStore.getWorkspaceIcons(workspace.id)"
        :id="item.id"
        :key="item.id"
        :item-type="item.itemType"
        :reference-id="item.referenceId"
        :initial-x="item.positionX"
        :initial-y="item.positionY"
        :label="item.label"
        :icon="item.icon"
        class="no-swipe"
        @position-changed="(id, x, y) => emit('position-changed', id, x, y)"
        @drag-start="(id, itemType, referenceId, w, h, x, y) => emit('icon-drag-start', id, itemType, referenceId, w, h, x, y)"
        @dragging="(id, x, y) => emit('icon-dragging', id, x, y)"
        @drag-end="emit('icon-drag-end')"
        @request-uninstall="(id) => emit('request-uninstall', id)"
      />

      <!-- Windows for this workspace - single instance, CSS-transformed in overview -->
      <HaexWindow
        v-for="window in workspaceWindows"
        v-show="windowManager.showWindowOverview || !window.isMinimized"
        :id="window.id"
        :key="window.id"
        v-model:x="window.x"
        v-model:y="window.y"
        v-model:width="window.width"
        v-model:height="window.height"
        :title="window.title"
        :icon="window.icon"
        :is-active="windowManager.isWindowActive(window.id)"
        :source-x="window.sourceX"
        :source-y="window.sourceY"
        :source-width="window.sourceWidth"
        :source-height="window.sourceHeight"
        :is-opening="window.isOpening"
        :is-closing="window.isClosing"
        :warning-level="
          window.type === 'extension' &&
          availableExtensions.find(
            (ext) => ext.id === window.sourceId,
          )?.devServerUrl
            ? 'warning'
            : undefined
        "
        class="no-swipe"
        :class="{
          'transition-opacity duration-300': !window.isNativeWebview,
          'opacity-0 pointer-events-none': windowManager.showWindowOverview && !window.isNativeWebview,
          'invisible': windowManager.showWindowOverview && window.isNativeWebview,
        }"
        @close="windowManager.closeWindow(window.id)"
        @minimize="windowManager.minimizeWindow(window.id)"
        @activate="windowManager.activateWindow(window.id)"
        @position-changed="
          (x, y) => windowManager.updateWindowPosition(window.id, x, y)
        "
        @size-changed="
          (width, height) =>
            windowManager.updateWindowSize(window.id, width, height)
        "
        @drag-start="emit('window-drag-start', window.id)"
        @drag-end="emit('window-drag-end')"
      >
        <!-- Render each tab's content (v-show keeps state alive across tab switches) -->
        <template
          v-for="tab in window.tabs"
          :key="tab.id"
        >
          <!-- System Window Tab -->
          <component
            :is="getSystemWindowComponent(tab.sourceId)"
            v-if="tab.type === 'system'"
            v-show="tab.id === window.activeTabId"
            :tab-id="tab.id"
            :is-dragging="windowManager.draggingWindowId === window.id"
            :window-params="tab.params"
            :category="tab.params?.category"
          />

          <!-- Native WebView Tab -->
          <div
            v-else-if="tab.isNativeWebview"
            v-show="tab.id === window.activeTabId"
            class="w-full h-full flex items-center justify-center bg-gray-50 dark:bg-gray-900"
          >
            <HaexIcon
              :name="tab.icon || 'i-lucide-app-window'"
              class="size-20"
            />
          </div>

          <!-- Extension iFrame Tab -->
          <HaexDesktopExtensionFrame
            v-else
            v-show="tab.id === window.activeTabId"
            :extension-id="tab.sourceId"
            :window-id="`${window.id}-${tab.id}`"
          />
        </template>
      </HaexWindow>
    </HaexDesktopWorkspaceDropZone>
  </UContextMenu>
</template>

<script setup lang="ts">
import type { CSSProperties } from 'vue'
import type { IWorkspace } from '~/stores/desktop/workspace'

interface DropTargetZone {
  x: number
  y: number
  width: number
  height: number
}

defineProps<{
  workspace: IWorkspace
  workspaceWindows: ReturnType<typeof useWindowManagerStore>['windows']
  isAreaSelecting: boolean
  selectionBoxStyle: CSSProperties
  showLeftSnapZone: boolean
  showRightSnapZone: boolean
  dropTargetZone: DropTargetZone | null
}>()

const emit = defineEmits<{
  'desktop-click': []
  'area-select-start': [event: MouseEvent]
  'drag-over': [event: DragEvent]
  'drop': [event: DragEvent, workspaceId: string]
  'position-changed': [id: string, x: number, y: number]
  'icon-drag-start': [
    id: string,
    itemType: string,
    referenceId: string,
    width: number,
    height: number,
    x: number,
    y: number,
  ]
  'icon-dragging': [id: string, x: number, y: number]
  'icon-drag-end': []
  'request-uninstall': [extensionId: string]
  'window-drag-start': [windowId: string]
  'window-drag-end': []
}>()

const desktopStore = useDesktopStore()
const extensionsStore = useExtensionsStore()
const windowManager = useWindowManagerStore()
const workspaceStore = useWorkspaceStore()

const { availableExtensions } = storeToRefs(extensionsStore)

// Get Vue Component for system window
const getSystemWindowComponent = (sourceId: string) => {
  const systemWindow = windowManager.getSystemWindow(sourceId)
  return systemWindow?.component
}
</script>
