import { invoke } from '@tauri-apps/api/core'

/**
 * Share-management actions for spaces: open the file/folder picker,
 * derive a sensible name, and register the share via the peer-storage store.
 *
 * Used by both the space list (+ button) and the space detail view so the
 * Android `content://` URI handling and toast/error wiring live in one place.
 */
export function useSpaceShares() {
  const store = usePeerStorageStore()
  const { add } = useToast()
  const { t } = useI18n({
    useScope: 'global',
    messages: {
      de: {
        spaceShares: {
          added: 'Freigabe hinzugefügt',
          removed: 'Freigabe entfernt',
          addFailed: 'Freigabe konnte nicht hinzugefügt werden',
          removeFailed: 'Freigabe konnte nicht entfernt werden',
        },
      },
      en: {
        spaceShares: {
          added: 'Share added',
          removed: 'Share removed',
          addFailed: 'Failed to add share',
          removeFailed: 'Failed to remove share',
        },
      },
    },
  })

  const extractFolderName = (path: string): string => {
    try {
      const parsed = JSON.parse(path)
      if (parsed.uri) {
        const decoded = decodeURIComponent(parsed.uri)
        const treeMatch = decoded.match(/tree\/[^:]+:(.+)/)
        if (treeMatch?.[1]) return treeMatch[1].split('/').pop() ?? 'Shared Folder'
        const lastSegment = decoded.split('/').pop() || decoded.split(':').pop()
        return lastSegment || 'Shared Folder'
      }
    } catch {
      // Not JSON — regular path
    }
    return path.split(/[/\\]/).pop() || 'Shared Folder'
  }

  const extractFileName = (path: string): string => {
    try {
      const parsed = JSON.parse(path)
      if (parsed.uri) {
        const decoded = decodeURIComponent(parsed.uri)
        const lastSegment = decoded.split('/').pop() || decoded.split(':').pop()
        return lastSegment || 'Shared File'
      }
    } catch {
      // Not JSON — regular path
    }
    return path.split(/[/\\]/).pop() || 'Shared File'
  }

  const addShareAsync = async (
    params: { spaceId: string, type: 'folder' | 'file' },
  ) => {
    const { spaceId, type } = params
    const selected =
      type === 'folder'
        ? await invoke<string | null>('filesystem_select_folder', {})
        : await invoke<string | null>('filesystem_select_file', {})
    if (!selected) return

    const name =
      type === 'folder' ? extractFolderName(selected) : extractFileName(selected)

    try {
      await store.addShareAsync(spaceId, name, selected)
      add({ title: t('spaceShares.added'), color: 'success' })
    } catch (error) {
      add({
        title: t('spaceShares.addFailed'),
        description: error instanceof Error ? error.message : String(error),
        color: 'error',
      })
    }
  }

  const removeShareAsync = async (shareId: string) => {
    try {
      await store.removeShareAsync(shareId)
      add({ title: t('spaceShares.removed'), color: 'neutral' })
    } catch (error) {
      add({
        title: t('spaceShares.removeFailed'),
        description: error instanceof Error ? error.message : String(error),
        color: 'error',
      })
    }
  }

  return {
    addShareAsync,
    removeShareAsync,
  }
}
