import { invoke } from '@tauri-apps/api/core'

export type ClipboardOperation = 'copy' | 'cut'

interface ClipboardEntry {
  name: string
  isDir: boolean
  absolutePath: string
}

interface ClipboardState {
  operation: ClipboardOperation
  sourcePath: string
  entries: ClipboardEntry[]
}

// Global state shared across all file browser instances
const clipboard = ref<ClipboardState | null>(null)

export function useFileClipboard() {
  const hasClipboard = computed(() => clipboard.value !== null && clipboard.value.entries.length > 0)
  const clipboardOperation = computed(() => clipboard.value?.operation)
  const clipboardCount = computed(() => clipboard.value?.entries.length ?? 0)
  const cutPaths = computed(() => {
    if (clipboard.value?.operation !== 'cut') return new Set<string>()
    return new Set(clipboard.value.entries.map(e => e.absolutePath))
  })

  const copy = (sourcePath: string, entries: ClipboardEntry[]) => {
    clipboard.value = { operation: 'copy', sourcePath, entries }
  }

  const cut = (sourcePath: string, entries: ClipboardEntry[]) => {
    clipboard.value = { operation: 'cut', sourcePath, entries }
  }

  const clear = () => {
    clipboard.value = null
  }

  const pasteAsync = async (targetDir: string) => {
    if (!clipboard.value) return

    const { operation, entries } = clipboard.value

    for (const entry of entries) {
      const dest = `${targetDir}/${entry.name}`

      // Always copy first
      if (entry.isDir) {
        await invoke('filesystem_copy_dir', { from: entry.absolutePath, to: dest })
      } else {
        await invoke('filesystem_copy', { from: entry.absolutePath, to: dest })
      }

      // Cut = copy + delete source
      if (operation === 'cut') {
        await invoke('filesystem_remove', { path: entry.absolutePath })
      }
    }

    if (operation === 'cut') {
      clipboard.value = null
    }
  }

  return {
    hasClipboard,
    clipboardOperation,
    cutPaths,
    clipboardCount,
    copy,
    cut,
    clear,
    pasteAsync,
  }
}
