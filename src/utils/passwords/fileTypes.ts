export type FileType = 'image' | 'pdf' | 'text' | 'other'

const IMAGE_EXTENSIONS = ['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'svg']
const TEXT_EXTENSIONS = ['txt', 'md', 'json', 'xml', 'csv', 'log', 'yml', 'yaml', 'ini', 'conf', 'config']

const MIME_TYPES: Record<string, string> = {
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  png: 'image/png',
  gif: 'image/gif',
  webp: 'image/webp',
  bmp: 'image/bmp',
  svg: 'image/svg+xml',
  pdf: 'application/pdf',
  txt: 'text/plain',
  md: 'text/markdown',
  json: 'application/json',
  xml: 'application/xml',
  csv: 'text/csv',
}

function getExtension(fileName: string): string {
  return fileName.toLowerCase().split('.').pop() ?? ''
}

export function getFileType(fileName: string): FileType {
  const ext = getExtension(fileName)
  if (IMAGE_EXTENSIONS.includes(ext)) return 'image'
  if (ext === 'pdf') return 'pdf'
  if (TEXT_EXTENSIONS.includes(ext)) return 'text'
  return 'other'
}

export function isImage(fileName: string): boolean {
  return getFileType(fileName) === 'image'
}

export function formatFileSize(bytes: number): string {
  if (bytes === 0) return '0 Bytes'
  const k = 1024
  const sizes = ['Bytes', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${Math.round((bytes / Math.pow(k, i)) * 100) / 100} ${sizes[i]}`
}

export function getMimeType(fileName: string): string {
  return MIME_TYPES[getExtension(fileName)] ?? 'application/octet-stream'
}

export function createDataUrl(base64Data: string, fileName: string): string {
  if (base64Data.startsWith('data:')) return base64Data
  return `data:${getMimeType(fileName)};base64,${base64Data}`
}
