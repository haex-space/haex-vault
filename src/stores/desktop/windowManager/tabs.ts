import type { Ref } from 'vue'
import type { IWindow, IWindowTab } from './state'
import { getSystemWindow } from './state'

export interface TabActionsDeps {
  windows: Ref<IWindow[]>
  activateWindow: (windowId: string) => void
  closeWindow: (windowId: string) => Promise<void> | void
}

export interface TabActions {
  isSourceSingleton: (type: 'system' | 'extension', sourceId: string) => boolean
  findSingletonTab: (
    type: 'system' | 'extension',
    sourceId: string,
  ) => { window: IWindow; tab: IWindowTab } | null
  activateSingletonTab: (
    win: IWindow,
    tab: IWindowTab,
    params?: Record<string, unknown>,
  ) => string
  syncWindowFromActiveTab: (win: IWindow) => void
  addTab: (windowId: string, tab: Omit<IWindowTab, 'id'>) => string | null
  addNewTabFromActive: (windowId: string) => string | null
  canAddTab: (windowId: string) => boolean
  switchTab: (windowId: string, tabId: string) => void
  closeTab: (windowId: string, tabId: string) => void
}

export function createTabActions(deps: TabActionsDeps): TabActions {
  const { windows, activateWindow, closeWindow } = deps

  /** Check if a source allows multiple instances. */
  const isSourceSingleton = (type: 'system' | 'extension', sourceId: string): boolean => {
    if (type === 'system') {
      return getSystemWindow(sourceId)?.singleton === true
    }
    const extensionsStore = useExtensionsStore()
    const ext = extensionsStore.availableExtensions.find(e => e.id === sourceId)
    return ext?.singleInstance === true
  }

  /** Find an existing open tab for a singleton source across all windows. */
  const findSingletonTab = (
    type: 'system' | 'extension',
    sourceId: string,
  ): { window: IWindow; tab: IWindowTab } | null => {
    for (const win of windows.value) {
      if (win.isClosing || !win.workspaceId) continue
      const tab = win.tabs.find(t => t.type === type && t.sourceId === sourceId)
      if (tab) return { window: win, tab }
    }
    return null
  }

  /** Sync the window's legacy fields from the active tab. */
  const syncWindowFromActiveTab = (win: IWindow) => {
    const tab = win.tabs.find(t => t.id === win.activeTabId)
    if (!tab) return
    win.type = tab.type
    win.sourceId = tab.sourceId
    win.title = tab.title
    win.icon = tab.icon
    win.params = tab.params
    win.isNativeWebview = tab.isNativeWebview
  }

  /** Switch to a singleton tab, activate its window, and optionally switch workspace. */
  const activateSingletonTab = (
    win: IWindow,
    tab: IWindowTab,
    params?: Record<string, unknown>,
  ): string => {
    win.activeTabId = tab.id
    syncWindowFromActiveTab(win)
    if (params) {
      tab.params = { ...tab.params, ...params }
    }
    const workspaceStore = useWorkspaceStore()
    if (win.workspaceId !== workspaceStore.currentWorkspace?.id) {
      workspaceStore.slideToWorkspace(win.workspaceId)
    }
    activateWindow(win.id)
    return win.id
  }

  /** Add a new tab to an existing window. Returns the tab ID or null. */
  const addTab = (windowId: string, tab: Omit<IWindowTab, 'id'>): string | null => {
    const win = windows.value.find(w => w.id === windowId)
    if (!win) return null

    // Singleton check: focus existing tab across all windows instead of creating a duplicate
    if (isSourceSingleton(tab.type, tab.sourceId)) {
      const found = findSingletonTab(tab.type, tab.sourceId)
      if (found) {
        activateSingletonTab(found.window, found.tab)
        return found.tab.id
      }
    }

    const tabId = crypto.randomUUID()
    win.tabs.push({ id: tabId, ...tab })
    win.activeTabId = tabId
    syncWindowFromActiveTab(win)
    return tabId
  }

  /** Add a new tab that duplicates the active tab's source (for the "+" button). */
  const addNewTabFromActive = (windowId: string): string | null => {
    const win = windows.value.find(w => w.id === windowId)
    if (!win) return null
    const activeTab = win.tabs.find(t => t.id === win.activeTabId)
    if (!activeTab) return null
    if (isSourceSingleton(activeTab.type, activeTab.sourceId)) return null

    return addTab(windowId, {
      type: activeTab.type,
      sourceId: activeTab.sourceId,
      title: activeTab.title,
      icon: activeTab.icon,
    })
  }

  /** Check if the "+" button should be shown (active source is not singleton). */
  const canAddTab = (windowId: string): boolean => {
    const win = windows.value.find(w => w.id === windowId)
    if (!win) return false
    const activeTab = win.tabs.find(t => t.id === win.activeTabId)
    if (!activeTab) return false
    return !isSourceSingleton(activeTab.type, activeTab.sourceId)
  }

  /** Switch to a specific tab. */
  const switchTab = (windowId: string, tabId: string) => {
    const win = windows.value.find(w => w.id === windowId)
    if (!win) return
    if (!win.tabs.some(t => t.id === tabId)) return
    win.activeTabId = tabId
    syncWindowFromActiveTab(win)
  }

  /** Close a tab. Last tab closes the window. */
  const closeTab = (windowId: string, tabId: string) => {
    const win = windows.value.find(w => w.id === windowId)
    if (!win) return

    if (win.tabs.length <= 1) {
      closeWindow(windowId)
      return
    }

    const tabIndex = win.tabs.findIndex(t => t.id === tabId)
    if (tabIndex === -1) return
    win.tabs.splice(tabIndex, 1)
    useNavigationStore().clearTabStacks(tabId)

    if (win.activeTabId === tabId) {
      const newIndex = Math.min(tabIndex, win.tabs.length - 1)
      win.activeTabId = win.tabs[newIndex]!.id
      syncWindowFromActiveTab(win)
    }
  }

  return {
    isSourceSingleton,
    findSingletonTab,
    activateSingletonTab,
    syncWindowFromActiveTab,
    addTab,
    addNewTabFromActive,
    canAddTab,
    switchTab,
    closeTab,
  }
}
