import { defineAsyncComponent, type Component } from 'vue'

export interface IWindowTab {
  id: string
  type: 'system' | 'extension'
  sourceId: string // extensionId or systemWindowId (depends on type)
  title: string
  icon?: string | null
  params?: Record<string, unknown>
  // Native webview window flag (separate OS window vs iframe)
  isNativeWebview?: boolean
}

export interface IWindow {
  id: string
  workspaceId: string // Window belongs to a specific workspace
  // Tab management
  tabs: IWindowTab[]
  activeTabId: string
  // Legacy fields (derived from active tab for backward compat)
  type: 'system' | 'extension'
  sourceId: string // extensionId or systemWindowId (depends on type)
  title: string
  icon?: string | null
  x: number
  y: number
  width: number
  height: number
  isMinimized: boolean
  zIndex: number
  // Animation source position (icon position)
  sourceX?: number
  sourceY?: number
  sourceWidth?: number
  sourceHeight?: number
  // Animation state
  isOpening?: boolean
  isClosing?: boolean
  // Native webview window flag (separate OS window vs iframe)
  isNativeWebview?: boolean
  // Optional parameters passed when opening the window
  params?: Record<string, unknown>
}

export interface SystemWindowDefinition {
  id: string
  name: string
  icon: string
  component: Component
  defaultWidth: number
  defaultHeight: number
  resizable?: boolean
  singleton?: boolean // Nur eine Instanz erlaubt?
}

export const SYSTEM_WINDOW_I18N_KEY_PREFIX = 'systemWindows'

export const systemWindows: Record<string, SystemWindowDefinition> = {
  settings: {
    id: 'settings',
    name: 'Settings',
    icon: 'i-mdi-cog',
    component: defineAsyncComponent(
      () => import('@/components/haex/system/settings/index.vue'),
    ),
    defaultWidth: 800,
    defaultHeight: 600,
    resizable: true,
    singleton: true,
  },
  files: {
    id: 'files',
    name: 'Files',
    icon: 'i-mdi-folder',
    component: defineAsyncComponent(
      () => import('@/components/haex/system/files/index.vue'),
    ),
    defaultWidth: 800,
    defaultHeight: 600,
    resizable: true,
    singleton: false,
  },
  marketplace: {
    id: 'marketplace',
    name: 'Marketplace',
    icon: 'i-mdi-store',
    component: defineAsyncComponent(
      () => import('@/components/haex/system/marketplace.vue'),
    ),
    defaultWidth: 1000,
    defaultHeight: 700,
    resizable: true,
    singleton: false,
  },
  passwords: {
    id: 'passwords',
    name: 'Passwords',
    icon: 'i-mdi-key-variant',
    component: defineAsyncComponent(
      () => import('@/components/haex/system/passwords/index.vue'),
    ),
    defaultWidth: 1000,
    defaultHeight: 700,
    resizable: true,
    singleton: true,
  },
}

export const getSystemWindow = (id: string): SystemWindowDefinition | undefined => {
  return systemWindows[id]
}

export const getAllSystemWindows = (): SystemWindowDefinition[] => {
  return Object.values(systemWindows)
}
