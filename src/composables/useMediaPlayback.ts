import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { exists as fileExists, mkdir } from '@tauri-apps/plugin-fs'
import { appCacheDir, join as joinPath } from '@tauri-apps/api/path'
import type { Ref } from 'vue'
import { getMediaType, type useFilePreview } from '~/composables/useFilePreview'
import {
  cacheKeyHash,
  toS3Prefix,
  type FileEntry,
  type RemotePeer,
} from '~/composables/fileBrowserHelpers'

/**
 * How a media file should be opened, by backend and media class.
 *
 * Audio/video always streams through the local HTTP range server
 * (`http://127.0.0.1:<port>/…`) — the only URL form WebKitGTK's GStreamer
 * backend accepts with Range support on every platform. `asset://` / blob
 * URLs either pre-buffer the whole file (audio) or fail to seek (video).
 */

/** Backend a media file lives on, refined enough to pick a playback path. */
export type MediaBackend = 's3' | 'p2p' | 'localFs' | 'localContentUri'

/** Concrete action for playing an audio/video file. */
export type AvAction =
  | 'streamLocal'
  | 'streamS3'
  | 'streamPeer'
  | 'streamContentUri'

/**
 * Classify the backend a file lives on from the selected peer and the
 * resolved local path. `localAbsPath` is only meaningful for local shares;
 * an Android Content URI is encoded as a JSON blob starting with `{`.
 */
export function classifyBackend(
  peer: { s3BackendId?: string; localPath?: string },
  localAbsPath: string | null,
): MediaBackend {
  if (peer.s3BackendId) return 's3'
  if (peer.localPath) {
    return localAbsPath?.startsWith('{') ? 'localContentUri' : 'localFs'
  }
  return 'p2p'
}

/**
 * Decide how to play an audio/video file given its backend. Every backend
 * streams through the local HTTP range server — Android Content URIs use a
 * dedicated source that seeks against the SAF file descriptor in a blocking
 * thread, so the full file never lands in RAM.
 */
export function resolveAvPlayback(backend: MediaBackend): AvAction {
  switch (backend) {
    case 'localFs':
      return 'streamLocal'
    case 's3':
      return 'streamS3'
    case 'p2p':
      return 'streamPeer'
    case 'localContentUri':
      return 'streamContentUri'
  }
}

interface MediaPlaybackDeps {
  selectedPeer: Ref<RemotePeer | null>
  currentPath: Ref<string>
  currentSpaceId: Ref<string | null>
  preview: ReturnType<typeof useFilePreview>
  resolveFilePath: (file: FileEntry & { searchPath?: string }) => string
  resolveLocalAbsolutePath: (file: FileEntry) => string | null
  startS3ChunkedDownload: (backendId: string, key: string, outputPath: string) => Promise<void>
}

/**
 * Playback + preview for the file browser. Opens media in the inline preview
 * modal; audio/video streams through the local HTTP range server (see
 * `resolveAvPlayback`), images/PDFs materialise a concrete path first.
 */
export function useMediaPlayback(deps: MediaPlaybackDeps) {
  const {
    selectedPeer,
    currentPath,
    currentSpaceId,
    preview,
    resolveFilePath,
    resolveLocalAbsolutePath,
    startS3ChunkedDownload,
  } = deps
  const peerStore = usePeerStorageStore()

  /**
   * Whether `file` can be "played" (image/video/audio/pdf preview) on the
   * current backend.
   */
  const canPlayFile = (file: FileEntry): boolean => {
    if (file.isDir) return false
    return getMediaType(file.name) !== 'unsupported'
  }

  /**
   * Materialise an S3 object in the app cache and return the local path.
   * Used for the image/pdf preview path (audio/video streams directly via
   * the range server instead). The download uses the same chunked /
   * resumable Rust command as `downloadFile`, so a previous partial cache
   * file is picked up where it left off.
   */
  const ensureS3FileCachedAsync = async (
    file: FileEntry,
    backendId: string,
  ): Promise<string> => {
    const key = toS3Prefix(currentPath.value) + file.name
    const cacheRoot = await appCacheDir()
    const subdir = await joinPath(cacheRoot, 's3-stream-cache')
    if (!(await fileExists(subdir))) {
      await mkdir(subdir, { recursive: true })
    }
    const hash = await cacheKeyHash(backendId, key)
    const cachePath = await joinPath(subdir, `${hash}-${file.name}`)
    await startS3ChunkedDownload(backendId, key, cachePath)
    return cachePath
  }

  /**
   * Unified open path for a file. Called both from single-click and the
   * "Play" context-menu item.
   *
   * Media (image/video/audio/pdf) opens in the inline preview modal so the
   * user gets immediate feedback inside the app. Everything else is handed
   * to the OS via `openWithSystem`.
   *
   * Audio/video — regardless of backend — streams through the local HTTP
   * range server (`http://127.0.0.1:<port>/…`), the only URL form WebKitGTK's
   * GStreamer backend accepts with Range support on every platform. The
   * backend → action mapping lives in `resolveAvPlayback`:
   *   - S3 / P2P / local share: register a streaming source and hand the
   *     element the range-server URL (no full-file download to disk first).
   *   - Android Content URI: register a SAF-fd-backed source so Range
   *     requests seek against the underlying file descriptor — keeps the
   *     full file out of RAM the same way the other backends do.
   *
   * Image / PDF / other still materialise a concrete path first:
   *   - S3: chunk-streamed to the app cache (resumable), then `openLocal`
   *     (image/pdf via `convertFileSrc`) or `openWithSystem`.
   *   - Local share: direct `openLocal` / `openWithSystem` on the share path.
   *   - P2P peer: `remoteReadAsync` into the peer cache, then `openLocal` /
   *     `openWithSystem`.
   */
  const playFile = async (file: FileEntry) => {
    const peer = selectedPeer.value
    if (!peer) return
    const type = getMediaType(file.name)
    const isMedia = type !== 'unsupported'

    // Audio/video: always stream through the local range server (or the
    // system player for Android Content URIs). UCAN + relayUrl for the P2P
    // case come from the same resolver `remoteReadAsync` uses, so capability
    // checks stay in lock-step.
    if (type === 'audio' || type === 'video') {
      const localAbsPath = peer.localPath ? resolveLocalAbsolutePath(file) : null
      const action = resolveAvPlayback(classifyBackend(peer, localAbsPath))
      try {
        switch (action) {
          case 'streamS3': {
            const key = toS3Prefix(currentPath.value) + file.name
            const url = await invoke<string>('media_server_register_s3_stream', {
              backendId: peer.s3BackendId,
              key,
            })
            preview.openStream(url, file.name)
            return
          }
          case 'streamPeer': {
            const path = resolveFilePath(file)
            const { ucanToken, relayUrl: deviceRelayUrl } =
              peerStore.resolveRequestContext(
                peer.endpointId,
                path,
                file.spaceId ?? currentSpaceId.value ?? undefined,
              )
            if (!ucanToken) {
              throw new Error('No valid UCAN token for this peer\'s space')
            }
            const url = await invoke<string>('media_server_register_peer_stream', {
              nodeId: peer.endpointId,
              relayUrl: deviceRelayUrl,
              path,
              ucanToken,
            })
            preview.openStream(url, file.name)
            return
          }
          case 'streamLocal': {
            const url = await invoke<string>('media_server_register', {
              path: localAbsPath,
            })
            preview.openStream(url, file.name)
            return
          }
          case 'streamContentUri': {
            // Android Content URI — `localAbsPath` is the file's FileUri JSON.
            // The native source seeks against the SAF fd inside spawn_blocking
            // for each Range, so the full file never lands in RAM.
            if (!localAbsPath) return
            const url = await invoke<string>('media_server_register_content_uri', {
              uriJson: localAbsPath,
              nameHint: file.name,
            })
            preview.openStream(url, file.name)
            return
          }
        }
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e)
        if (msg.includes('cancelled')) return
        throw e
      }
    }

    let absPath: string | null = null
    try {
      if (peer.s3BackendId) {
        absPath = await ensureS3FileCachedAsync(file, peer.s3BackendId)
      } else if (peer.localPath) {
        absPath = resolveLocalAbsolutePath(file)
      } else {
        absPath = await peerStore.remoteReadAsync(
          peer.endpointId,
          resolveFilePath(file),
          undefined,
          file.spaceId ?? currentSpaceId.value ?? undefined,
        )
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      if (msg.includes('cancelled')) return
      throw e
    }
    if (!absPath) return

    if (isMedia) {
      await preview.openLocal(absPath, file.name, file.size)
    } else {
      await preview.openWithSystem(absPath)
    }
  }

  const getThumbnailUrl = (file: FileEntry): string | null => {
    if (file.isDir) return null
    if (getMediaType(file.name) !== 'image') return null
    if (!selectedPeer.value?.localPath) return null
    const absPath = resolveLocalAbsolutePath(file)
    if (!absPath || absPath.startsWith('{')) return null
    return convertFileSrc(absPath)
  }

  return { canPlayFile, playFile, getThumbnailUrl }
}
