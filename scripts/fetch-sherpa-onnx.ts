/**
 * Downloads the pinned sherpa-onnx libraries and checks their SHA-256 (spec 066, T001).
 *
 *   node scripts/fetch-sherpa-onnx.ts                     # the host's desktop target
 *   node scripts/fetch-sherpa-onnx.ts --target <triple>…  # e.g. aarch64-linux-android
 *   node scripts/fetch-sherpa-onnx.ts --soft              # warn instead of failing (postinstall)
 *
 * Cargo reads the archives from `SHERPA_ONNX_ARCHIVE_DIR` (root `.cargo/config.toml`); without
 * them a build with the `voice` feature stops with "does not contain expected archive". An archive
 * that is already there with the right digest is not downloaded again.
 */
import { createHash } from 'node:crypto'
import { createReadStream, existsSync } from 'node:fs'
import { mkdir, rename, rm, writeFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'

import {
  ARCHIVE_DIR,
  archiveFor,
  downloadUrl,
  hostTarget,
  type SherpaArchive,
} from './lib/sherpaOnnxArchives.ts'
import { readArchiveResponse } from './lib/sherpaOnnxDownload.ts'

const root = resolve(import.meta.dirname, '..')
const dir = join(root, ARCHIVE_DIR)

const targets: string[] = []
let soft = false
const args = process.argv.slice(2)
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--target' && args[i + 1]) targets.push(args[++i]!)
  else if (args[i] === '--soft') soft = true
  else {
    console.error(
      'usage: node scripts/fetch-sherpa-onnx.ts [--soft] [--target <triple>]…',
    )
    process.exit(2)
  }
}
if (targets.length === 0) targets.push(hostTarget())

async function sha256Of(file: string): Promise<string> {
  const hash = createHash('sha256')
  for await (const chunk of createReadStream(file)) hash.update(chunk)
  return hash.digest('hex')
}

async function fetchOne(archive: SherpaArchive): Promise<void> {
  const file = join(dir, archive.name)
  if (existsSync(file) && (await sha256Of(file)) === archive.sha256) {
    console.log(`ok      ${archive.name}`)
    return
  }
  const url = downloadUrl(archive)
  console.log(`fetch   ${url}`)
  const response = await fetch(url)
  if (!response.ok) {
    throw new Error(`${url} answered ${response.status}`)
  }
  const body = await readArchiveResponse(response, archive)
  const digest = createHash('sha256').update(body).digest('hex')
  if (digest !== archive.sha256) {
    throw new Error(
      `${archive.name}: SHA-256 ${digest} does not match the pinned ${archive.sha256}`,
    )
  }
  const partial = `${file}.partial`
  await writeFile(partial, body)
  await rename(partial, file)
  console.log(`ok      ${archive.name}`)
}

await mkdir(dir, { recursive: true })
const unique = new Map(targets.map((t) => [archiveFor(t).name, archiveFor(t)]))
try {
  for (const archive of unique.values()) await fetchOne(archive)
} catch (error) {
  for (const archive of unique.values()) {
    await rm(join(dir, `${archive.name}.partial`), { force: true })
  }
  console.error(error instanceof Error ? error.message : error)
  if (soft) {
    console.error(
      'sherpa-onnx archives missing; builds with the `voice` feature need `pnpm fetch:sherpa`',
    )
  } else {
    process.exit(1)
  }
}
