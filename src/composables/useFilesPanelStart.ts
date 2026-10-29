import type { SourceRef } from '@bindings/SourceRef'
import type { FilesPanelContext } from '~/composables/useFilesSplit'
import { isPeerStorage } from '~/lib/files/registry'

/**
 * The start of a panel (no path yet): the home folder, or the first drive where there is none; a
 * storage starts at its root, another own device at its home folder, as this one does (spec 056).
 */
export function useFilesPanelStart(
  context: FilesPanelContext,
  source: Ref<SourceRef>,
  path: Ref<string | null>,
) {
  const { peerRootsAsync } = useFiles()

  watchEffect(() => {
    if (path.value !== null) return
    if (source.value.kind === 'storage') {
      if (isPeerStorage(source.value.storageId)) {
        void openPeerStartAsync(source.value.storageId)
        return
      }
      context.initialize({ source: source.value, path: '/' })
      return
    }
    const data = context.sources.value
    if (!data) return
    const home = data.known.find((known) => known.name === 'home')?.path
    const start =
      home ??
      data.drives.find((drive) => drive.state === 'mounted')?.path ??
      undefined
    if (start) context.initialize({ source: source.value, path: start })
  })

  async function openPeerStartAsync(storageId: string) {
    let start = '/'
    try {
      const roots = await peerRootsAsync(storageId)
      start =
        roots.known.find((known) => known.name === 'home')?.path ??
        roots.drives[0]?.path ??
        '/'
    } catch {
      // Not reachable now: the root shows why when it fails to list.
    }
    const now = source.value
    if (
      path.value === null &&
      now.kind === 'storage' &&
      now.storageId === storageId
    )
      context.initialize({ source: now, path: start })
  }
}
