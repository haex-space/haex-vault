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
export type AvAction = 'streamLocal' | 'streamS3' | 'streamPeer' | 'openSystem'

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
 * streams through the range server except Android Content URIs, which have
 * no file path for the server and must never be loaded into RAM — the system
 * player streams those from disk until a content-URI streaming source exists.
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
      return 'openSystem'
  }
}
