import { describe, it, expect } from 'vitest'
import { classifyBackend, resolveAvPlayback } from '~/composables/useMediaPlayback'

describe('classifyBackend', () => {
  it('classifies an S3 peer as s3', () => {
    expect(classifyBackend({ s3BackendId: 'b1' }, null)).toBe('s3')
  })

  it('classifies a local share with a real fs path as localFs', () => {
    expect(
      classifyBackend({ localPath: '/home/u/share' }, '/home/u/share/song.mp3'),
    ).toBe('localFs')
  })

  it('classifies a local share resolving to an Android content URI as localContentUri', () => {
    expect(
      classifyBackend({ localPath: 'content://x' }, '{"uri":"content://x"}'),
    ).toBe('localContentUri')
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

  // Android content URIs have no file path for the range server and must
  // NEVER be base64-loaded into RAM (would OOM on large media). The system
  // player streams from disk until the Phase-2 content-URI source exists.
  it('opens Android content-URI media with the system player', () => {
    expect(resolveAvPlayback('localContentUri')).toBe('openSystem')
  })

  it('streams S3 media through the range server', () => {
    expect(resolveAvPlayback('s3')).toBe('streamS3')
  })

  it('streams P2P peer media through the range server', () => {
    expect(resolveAvPlayback('p2p')).toBe('streamPeer')
  })
})
