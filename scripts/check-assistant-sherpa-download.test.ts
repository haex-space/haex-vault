import assert from 'node:assert/strict'
import { test } from 'node:test'

import { readArchiveResponse } from './lib/sherpaOnnxDownload.ts'

const archive = { name: 'test.tar.bz2', sha256: '0'.repeat(64), size: 4 }

test('reads an archive response whose body matches its pinned size', async () => {
  const body = await readArchiveResponse(
    new Response(new Uint8Array([1, 2, 3, 4])),
    archive,
  )
  assert.deepEqual(body, Buffer.from([1, 2, 3, 4]))
})

test('rejects an archive response as soon as it exceeds its pinned size', async () => {
  await assert.rejects(
    readArchiveResponse(new Response(new Uint8Array([1, 2, 3, 4, 5])), archive),
    /response exceeds 4 bytes/,
  )
})

test('rejects a truncated archive response', async () => {
  await assert.rejects(
    readArchiveResponse(new Response(new Uint8Array([1, 2, 3])), archive),
    /expected 4 bytes, received 3/,
  )
})
