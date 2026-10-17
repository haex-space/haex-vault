// Pure helpers of the import wizard (spec 034-password-manager, US7): grouping the report rows by
// entry, the progress as a percentage, the reason of a failed run, a file name from a path. No
// value of a secret passes through here; rows carry titles, paths and names only.

export type ReportRow = {
  itemId?: string | null
  title: string
  folderPath: string
  kind: string
  field?: string | null
  fileName?: string | null
  sizeMib?: number | null
}

export type ReportGroup<R extends ReportRow = ReportRow> = {
  /** Stable key for a list. */
  key: string
  /** The entry to open; `null` for the source as a whole or an entry that was not written. */
  itemId: string | null
  title: string
  folderPath: string
  rows: R[]
}

/** Groups the rows by entry in the order they first appear. Rows without an entry and without a
 * title are about the source itself and form one group `source`. */
export function groupReport<R extends ReportRow>(
  rows: readonly R[],
): ReportGroup<R>[] {
  const groups = new Map<string, ReportGroup<R>>()
  for (const row of rows) {
    const itemId = row.itemId ?? null
    const key =
      itemId ??
      (row.title === '' && row.folderPath === ''
        ? 'source'
        : `${row.title}\u0000${row.folderPath}`)
    let group = groups.get(key)
    if (!group) {
      group = {
        key,
        itemId,
        title: row.title,
        folderPath: row.folderPath,
        rows: [],
      }
      groups.set(key, group)
    }
    group.rows.push(row)
  }
  return [...groups.values()]
}

/** `done` of `total` as a whole percentage; `null` while the total is not known. */
export function progressPercent(done: number, total: number): number | null {
  if (!Number.isFinite(done) || !Number.isFinite(total) || total <= 0)
    return null
  return Math.max(0, Math.min(100, Math.round((done / total) * 100)))
}

/** What a source asks for besides the file: KeePass a password and/or a key file, haex-vault 2.x (Legacy) its
 * vault password (spec 037), the exports nothing. */
export function sourceSecrets(source: string): {
  password: boolean
  keyFile: boolean
} {
  return {
    password: source === 'keepass' || source === 'legacyhaexvault',
    keyFile: source === 'keepass',
  }
}

/** Whether the preview can start: a file, and what the source needs to open it (KeePass one of
 * password or key file, haex-vault 2.x its password). */
export function canPreviewImport(
  source: string,
  hasFile: boolean,
  hasPassword: boolean,
  hasKeyFile: boolean,
): boolean {
  if (!hasFile) return false
  if (source === 'keepass') return hasPassword || hasKeyFile
  if (source === 'legacyhaexvault') return hasPassword
  return true
}

/** The `reason` of an `ImportFailed` error (`cancelled`, `wrong_credentials`, …), else `null`. */
export function importFailureReason(error: unknown): string | null {
  if (
    error &&
    typeof error === 'object' &&
    (error as { kind?: unknown }).kind === 'PasswordsImportFailed'
  ) {
    const reason = (error as { reason?: unknown }).reason
    return typeof reason === 'string' ? reason : null
  }
  return null
}
