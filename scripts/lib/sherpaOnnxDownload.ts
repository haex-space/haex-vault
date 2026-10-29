import type { SherpaArchive } from './sherpaOnnxArchives.ts'

/** Reads an archive response only up to its pinned release-asset size. */
export async function readArchiveResponse(
  response: Response,
  archive: SherpaArchive,
): Promise<Buffer> {
  const reader = response.body?.getReader()
  if (!reader) throw new Error(`${archive.name}: response has no body`)
  const chunks: Buffer[] = []
  let size = 0
  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      size += value.byteLength
      if (size > archive.size) {
        await reader.cancel()
        throw new Error(`${archive.name}: response exceeds ${archive.size} bytes`)
      }
      chunks.push(Buffer.from(value))
    }
  } finally {
    reader.releaseLock()
  }
  if (size !== archive.size) {
    throw new Error(`${archive.name}: expected ${archive.size} bytes, received ${size}`)
  }
  return Buffer.concat(chunks, size)
}
