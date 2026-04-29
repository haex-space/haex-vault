import { eq } from 'drizzle-orm'
import {
  haexPasswordsItemSnapshots,
  haexPasswordsSnapshotBinaries,
  haexPasswordsItemBinaries,
} from '~/database/schemas'
import { requireDb } from '~/stores/vault'

export interface SnapshotData {
  title: string
  username: string | null
  password: string | null
  url: string | null
  note: string | null
  icon: string | null
  color: string | null
  expiresAt: string | null
  otpSecret: string | null
  tagNames: string[]
  keyValues: Array<{ key: string; value: string }>
  attachments: Array<{ fileName: string; binaryHash: string }>
}

export async function createSnapshotAsync(
  itemId: string,
  data: SnapshotData,
  modifiedAt: string,
): Promise<void> {
  const db = requireDb()

  const snapshotId = crypto.randomUUID()
  await db.insert(haexPasswordsItemSnapshots).values({
    id: snapshotId,
    itemId,
    snapshotData: JSON.stringify(data),
    modifiedAt,
  })

  if (data.attachments.length > 0) {
    await db.insert(haexPasswordsSnapshotBinaries).values(
      data.attachments.map((att) => ({
        snapshotId,
        binaryHash: att.binaryHash,
        fileName: att.fileName,
      })),
    )
  }
}

export async function loadSnapshotsAsync(itemId: string) {
  const db = requireDb()
  return db
    .select()
    .from(haexPasswordsItemSnapshots)
    .where(eq(haexPasswordsItemSnapshots.itemId, itemId))
}

export async function loadSnapshotAttachmentsAsync(snapshotId: string) {
  const db = requireDb()
  return db
    .select()
    .from(haexPasswordsSnapshotBinaries)
    .where(eq(haexPasswordsSnapshotBinaries.snapshotId, snapshotId))
}

export async function loadCurrentAttachmentsAsSnapshotRefs(
  itemId: string,
): Promise<Array<{ fileName: string; binaryHash: string }>> {
  const db = requireDb()
  const rows = await db
    .select()
    .from(haexPasswordsItemBinaries)
    .where(eq(haexPasswordsItemBinaries.itemId, itemId))
  return rows.map((r) => ({ fileName: r.fileName, binaryHash: r.binaryHash }))
}
