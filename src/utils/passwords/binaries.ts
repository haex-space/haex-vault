import { eq, notInArray } from 'drizzle-orm'
import {
  haexPasswordsBinaries,
  haexPasswordsItemBinaries,
  haexPasswordsSnapshotBinaries,
} from '~/database/schemas'
import { requireDb } from '~/stores/vault'

export function arrayBufferToBase64(arrayBuffer: ArrayBuffer): string {
  const uint8Array = new Uint8Array(arrayBuffer)
  let binary = ''
  for (let i = 0; i < uint8Array.length; i++) {
    binary += String.fromCharCode(uint8Array[i]!)
  }
  return btoa(binary)
}

export async function calculateBinaryHashAsync(
  data: string | Uint8Array | ArrayBuffer,
): Promise<string> {
  let bytes: Uint8Array
  if (typeof data === 'string') {
    const binaryString = atob(data)
    bytes = new Uint8Array(binaryString.length)
    for (let i = 0; i < binaryString.length; i++) {
      bytes[i] = binaryString.charCodeAt(i)
    }
  } else {
    bytes = new Uint8Array(data)
  }

  const properBuffer = new ArrayBuffer(bytes.byteLength)
  new Uint8Array(properBuffer).set(bytes)

  const hashBuffer = await crypto.subtle.digest('SHA-256', properBuffer)
  return Array.from(new Uint8Array(hashBuffer))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
}

export async function addBinaryAsync(
  data: string,
  size: number | bigint,
  type: 'icon' | 'attachment' = 'attachment',
): Promise<string> {
  const sizeNumber = Number(size)
  const db = requireDb()
  const hash = await calculateBinaryHashAsync(data)

  const existing = await db
    .select({ hash: haexPasswordsBinaries.hash })
    .from(haexPasswordsBinaries)
    .where(eq(haexPasswordsBinaries.hash, hash))
    .limit(1)

  if (existing.length === 0) {
    await db
      .insert(haexPasswordsBinaries)
      .values({ hash, data, size: sizeNumber, type })
  }

  return hash
}

export async function pruneOrphanedBinariesAsync(): Promise<void> {
  const db = requireDb()

  const [itemRefs, snapshotRefs] = await Promise.all([
    db.selectDistinct({ hash: haexPasswordsItemBinaries.binaryHash }).from(haexPasswordsItemBinaries),
    db.selectDistinct({ hash: haexPasswordsSnapshotBinaries.binaryHash }).from(haexPasswordsSnapshotBinaries),
  ])

  const referenced = [
    ...new Set([...itemRefs.map((r) => r.hash), ...snapshotRefs.map((r) => r.hash)]),
  ]

  if (referenced.length === 0) {
    await db.delete(haexPasswordsBinaries)
  } else {
    await db.delete(haexPasswordsBinaries).where(notInArray(haexPasswordsBinaries.hash, referenced))
  }
}
