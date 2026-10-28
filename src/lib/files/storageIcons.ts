// The icon of a cloud storage in the file browser's sidebar by the protocol of its connection
// (spec 038, 050), so S3, WebDAV and SFTP storages tell apart at a glance. Kept as plain strings
// in a .ts file, which the icon bundle scan reads.
export type StorageProtocol = 's3' | 'webdav' | 'sftp'

const ICONS: Record<StorageProtocol, string> = {
  s3: 'lucide:database',
  webdav: 'lucide:globe',
  sftp: 'lucide:terminal',
}

/** The icon for `protocol`; the generic cloud while the connection is not known (yet). */
export function storageIcon(protocol: string | null | undefined): string {
  return protocol && protocol in ICONS
    ? ICONS[protocol as StorageProtocol]
    : 'lucide:cloud'
}
