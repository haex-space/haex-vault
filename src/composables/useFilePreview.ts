import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { openPath } from '@tauri-apps/plugin-opener'
import { isDesktop } from '~/utils/platform'

const IMAGE_EXTS = new Set(['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'bmp', 'ico'])
const VIDEO_EXTS = new Set(['mp4', 'mov', 'webm', 'ogv'])
const AUDIO_EXTS = new Set(['mp3', 'wav', 'flac', 'ogg', 'aac', 'm4a'])
const PDF_EXTS = new Set(['pdf'])

const MIME_TYPES: Record<string, string> = {
  jpg: 'image/jpeg', jpeg: 'image/jpeg', png: 'image/png', gif: 'image/gif',
  webp: 'image/webp', svg: 'image/svg+xml', bmp: 'image/bmp',
  mp4: 'video/mp4', mov: 'video/quicktime', webm: 'video/webm',
  mp3: 'audio/mpeg', wav: 'audio/wav', flac: 'audio/flac',
  ogg: 'audio/ogg', aac: 'audio/aac', m4a: 'audio/mp4',
  pdf: 'application/pdf',
}

export type MediaType = 'image' | 'video' | 'audio' | 'pdf' | 'unsupported'

export function getMediaType(filename: string): MediaType {
  const ext = filename.split('.').pop()?.toLowerCase() || ''
  if (IMAGE_EXTS.has(ext)) return 'image'
  if (VIDEO_EXTS.has(ext)) return 'video'
  if (AUDIO_EXTS.has(ext)) return 'audio'
  if (PDF_EXTS.has(ext)) return 'pdf'
  return 'unsupported'
}

export function isPreviewable(filename: string): boolean {
  return getMediaType(filename) !== 'unsupported'
}

export function useFilePreview() {
  const previewUrl = ref<string | null>(null)
  const previewFilename = ref<string | null>(null)
  const previewType = ref<MediaType>('unsupported')
  const previewLoading = ref(false)
  const isOpen = computed(() => previewFilename.value !== null)

  const cleanup = () => {
    if (previewUrl.value?.startsWith('blob:')) {
      URL.revokeObjectURL(previewUrl.value)
    }
    previewUrl.value = null
    previewFilename.value = null
    previewType.value = 'unsupported'
  }

  /**
   * Open preview for a local file (own device share).
   * On desktop uses convertFileSrc (zero-copy), on mobile reads via base64.
   */
  const openLocal = async (absolutePath: string, filename: string) => {
    cleanup()
    previewFilename.value = filename
    previewType.value = getMediaType(filename)
    previewLoading.value = true

    try {
      if (isDesktop()) {
        previewUrl.value = convertFileSrc(absolutePath)
      } else {
        const base64 = await invoke<string>('filesystem_read_file', { path: absolutePath })
        previewUrl.value = base64ToObjectUrl(base64, filename)
      }
    } finally {
      previewLoading.value = false
    }
  }

  /**
   * Open preview for a remote file (fetched via iroh P2P).
   * Receives base64 data and creates a blob URL.
   */
  const openRemote = async (base64: string, filename: string) => {
    cleanup()
    previewFilename.value = filename
    previewType.value = getMediaType(filename)
    previewLoading.value = true

    try {
      previewUrl.value = base64ToObjectUrl(base64, filename)
    } finally {
      previewLoading.value = false
    }
  }

  const close = () => {
    cleanup()
  }

  /**
   * Trigger a browser download for a base64 string
   */
  const downloadBase64 = (base64: string, filename: string) => {
    const url = base64ToObjectUrl(base64, filename)
    triggerDownload(url, filename)
    URL.revokeObjectURL(url)
  }

  const openWithSystem = async (absolutePath: string) => {
    await openPath(absolutePath)
  }

  return {
    previewUrl: readonly(previewUrl),
    previewFilename: readonly(previewFilename),
    previewType: readonly(previewType),
    previewLoading: readonly(previewLoading),
    isOpen,
    openLocal,
    openRemote,
    close,
    downloadBase64,
    openWithSystem,
    getMediaType,
    isPreviewable,
  }
}

function base64ToObjectUrl(base64: string, filename: string): string {
  const binaryString = atob(base64)
  const bytes = new Uint8Array(binaryString.length)
  for (let i = 0; i < binaryString.length; i++) {
    bytes[i] = binaryString.charCodeAt(i)
  }
  const ext = filename.split('.').pop()?.toLowerCase() || ''
  const mime = MIME_TYPES[ext] || 'application/octet-stream'
  const blob = new Blob([bytes], { type: mime })
  return URL.createObjectURL(blob)
}

function triggerDownload(url: string, filename: string) {
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
}
