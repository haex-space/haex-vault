import type { SelectHaexPasswordsItemBinaries } from '~/database/schemas'

export interface AttachmentWithSize extends SelectHaexPasswordsItemBinaries {
  /** File size in bytes — present for new (pre-save) attachments, absent for DB-loaded ones. */
  size?: number
  /** Base64 data string (with or without data-URL prefix) — only set before the attachment is saved to DB. */
  data?: string
}
