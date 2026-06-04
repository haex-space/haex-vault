import { describe, it, expect, vi, beforeEach } from 'vitest'

// `useMediaPlayback` pulls in `~/utils/platform`, which calls `platform()` from
// `@tauri-apps/plugin-os` at runtime — undefined in vitest. Stub it before
// importing the module under test.
vi.mock('~/utils/platform', () => ({
  isAndroid: vi.fn(() => false),
}))

const { classifyBackend, resolveAvPlayback } = await import('~/composables/useMediaPlayback')
const { isAndroid } = await import('~/utils/platform')

describe('classifyBackend', () => {
  beforeEach(() => {
    vi.mocked(isAndroid).mockReturnValue(false)
  })

  it('classifies an S3 peer as s3', () => {
    expect(classifyBackend({ s3BackendId: 'b1' }, null)).toBe('s3')
  })

  it('classifies a local share with a real fs path as localFs', () => {
    expect(
      classifyBackend({ localPath: '/home/u/share' }, '/home/u/share/song.mp3'),
    ).toBe('localFs')
  })

  it('on Android, a local share resolving to a content-URI JSON blob is localContentUri', () => {
    vi.mocked(isAndroid).mockReturnValue(true)
    expect(
      classifyBackend({ localPath: 'content://x' }, '{"uri":"content://x"}'),
    ).toBe('localContentUri')
  })

  it('off-Android, a `{`-prefixed path falls back to localFs (Android-only command would not exist)', () => {
    vi.mocked(isAndroid).mockReturnValue(false)
    expect(
      classifyBackend({ localPath: 'content://x' }, '{"uri":"content://x"}'),
    ).toBe('localFs')
  })

  it('classifies a peer with neither s3 nor localPath as p2p', () => {
    expect(classifyBackend({}, null)).toBe('p2p')
  })
})

describe('resolveAvPlayback', () => {
  // The bug: local filesystem audio/video fell through to convertFileSrc
  // (asset://), which WebKitGTK's GStreamer rejects — MP3 pre-downloaded,
  // MP4 never played. It must stream through the local HTTP range server.
  it('streams local filesystem media through the range server', () => {
    expect(resolveAvPlayback('localFs')).toBe('streamLocal')
  })

  // Android content URIs stream through the range server via a dedicated
  // SAF-fd source — Range requests seek against the underlying fd in a
  // blocking thread, so the full file never lands in RAM.
  it('streams Android content-URI media via the SAF-fd source', () => {
    expect(resolveAvPlayback('localContentUri')).toBe('streamContentUri')
  })

  it('streams S3 media through the range server', () => {
    expect(resolveAvPlayback('s3')).toBe('streamS3')
  })

  it('streams P2P peer media through the range server', () => {
    expect(resolveAvPlayback('p2p')).toBe('streamPeer')
  })
})
