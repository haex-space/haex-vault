// A real model for the scenarios that run one on the device (spec 070 T040): the catalog's smallest,
// Qwen3 0.6B, pinned to a revision and checked by its SHA-256, the file `llamacpp-platforms.yml`
// loads too. It is fetched once into a cache folder of this machine (`E2E_MODEL_CACHE`, else
// `~/.cache/holzi-e2e/models`), which CI keeps between runs.
import { createHash } from 'node:crypto'
import {
  createReadStream,
  createWriteStream,
  existsSync,
  mkdirSync,
  renameSync,
  rmSync,
} from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { Readable, Transform } from 'node:stream'
import { pipeline } from 'node:stream/promises'

export interface PinnedModel {
  /** The catalog id (`src-tauri/src/catalog/model_catalog.json`). */
  id: string
  name: string
  tokenizerRepo: string
  filename: string
  url: string
  sha256: string
}

export const QWEN3_0_6B: PinnedModel = {
  id: 'qwen3-0.6b-instruct-q4_k_m',
  name: 'Qwen3 0.6B Instruct (Q4_K_M)',
  tokenizerRepo: 'Qwen/Qwen3-0.6B',
  filename: 'Qwen_Qwen3-0.6B-Q4_K_M.gguf',
  url: 'https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF/resolve/60b85c0e3d8fe0f6474f406922a26d12aca4550d/Qwen_Qwen3-0.6B-Q4_K_M.gguf',
  sha256: '9acfc1e001311f34b4252001b626f2e466d592a42065f66571bff3790d4e1b14',
}

async function sha256Of(path: string): Promise<string> {
  const hash = createHash('sha256')
  await pipeline(createReadStream(path), hash)
  return hash.digest('hex')
}

/** The path of `model` on this machine, fetched first when the cache does not hold it. */
export async function pinnedModelFile(
  model: PinnedModel,
  env: NodeJS.ProcessEnv = process.env,
): Promise<string> {
  const folder =
    env.E2E_MODEL_CACHE || join(homedir(), '.cache', 'holzi-e2e', 'models')
  mkdirSync(folder, { recursive: true })
  const path = join(folder, `${model.sha256}-${model.filename}`)
  if (existsSync(path)) {
    if ((await sha256Of(path)) === model.sha256) return path
    rmSync(path, { force: true })
  }
  const response = await fetch(model.url)
  if (!response.ok || response.body === null) {
    throw new Error(`fetching ${model.url}: HTTP ${response.status}`)
  }
  const partial = `${path}.part`
  const hash = createHash('sha256')
  const hashing = new Transform({
    transform(chunk: Buffer, _encoding, done) {
      hash.update(chunk)
      done(null, chunk)
    },
  })
  try {
    await pipeline(
      Readable.fromWeb(response.body),
      hashing,
      createWriteStream(partial),
    )
    const actual = hash.digest('hex')
    if (actual !== model.sha256) {
      throw new Error(
        `${model.url} has SHA-256 ${actual}, expected ${model.sha256}`,
      )
    }
    renameSync(partial, path)
  } finally {
    rmSync(partial, { force: true })
  }
  return path
}
