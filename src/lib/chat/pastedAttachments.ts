// What a paste into the composer carries that can become an attachment (spec 011 FR-019).
// Kept free of Tauri and Vue so a Node check can load it.

/** The files a paste brings: bytes the webview hands over, or paths of files copied in the
 * system's file manager. */
export interface PastedFiles {
  files: File[]
  paths: string[]
}

/** The part of `DataTransfer` this reads. */
interface PasteData {
  files: ArrayLike<File>
  getData: (format: string) => string
}

/** The extensions `chat/attachments.rs` recognizes, by the type the webview reports. */
const EXTENSION_BY_TYPE: Record<string, string> = {
  'image/png': 'png',
  'image/jpeg': 'jpg',
  'image/webp': 'webp',
  'image/gif': 'gif',
  'application/pdf': 'pdf',
  'text/plain': 'txt',
  'text/markdown': 'md',
}

/** The name an attachment is listed under. A clipboard image is usually just `image.png`, but one
 * without an extension gets the one its type stands for, since the backend tells the kind of an
 * attachment by that. */
export function pastedFileName(file: { name: string; type: string }): string {
  const name = file.name.trim() || 'pasted'
  if (/\.[^./\\]+$/.test(name)) return name
  const extension = EXTENSION_BY_TYPE[file.type]
  return extension ? `${name}.${extension}` : name
}

/** The `file:` addresses of a `text/uri-list`, or none when it holds anything else: a list of web
 * addresses is a copied link, not files, and a paste of that stays a plain paste. */
export function filePathsFromUriList(list: string): string[] {
  const lines = list
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith('#'))
  if (!lines.length || !lines.every((line) => line.startsWith('file://'))) {
    return []
  }
  return lines
}

/** The files in a paste; both lists are empty for a paste of plain text. Files copied in a file
 * manager are taken by their path (nothing is copied through the window), anything else by its
 * bytes. */
export function readPaste(data: PasteData | null | undefined): PastedFiles {
  if (!data) return { files: [], paths: [] }
  const paths = filePathsFromUriList(data.getData('text/uri-list'))
  if (paths.length) return { files: [], paths }
  return { files: Array.from(data.files), paths: [] }
}

export function hasPastedFiles(paste: PastedFiles): boolean {
  return paste.files.length > 0 || paste.paths.length > 0
}
