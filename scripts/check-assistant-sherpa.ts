import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { test } from 'node:test'

import {
  ARCHIVE_DIR,
  archiveFor,
  downloadUrl,
  hostTarget,
  knownTargets,
  SHERPA_ONNX_VERSION,
} from './lib/sherpaOnnxArchives.ts'

const root = resolve(import.meta.dirname, '..')
const read = (path: string) => readFileSync(resolve(root, path), 'utf8')

test('the archive pins match the exact crate pin in Cargo.toml', () => {
  const manifest = read('src-tauri/Cargo.toml')
  const pin = manifest.match(/^sherpa-onnx = \{ version = "=([^"]+)"/m)
  assert.ok(pin, 'sherpa-onnx is pinned exactly in src-tauri/Cargo.toml')
  assert.equal(pin[1], SHERPA_ONNX_VERSION)
})

test('Cargo.lock resolves both sherpa crates to the pinned version', () => {
  const lock = read('src-tauri/Cargo.lock')
  for (const name of ['sherpa-onnx', 'sherpa-onnx-sys']) {
    const entry = lock.match(
      new RegExp(`name = "${name}"\\nversion = "([^"]+)"`),
    )
    assert.ok(entry, `${name} is in Cargo.lock`)
    assert.equal(entry[1], SHERPA_ONNX_VERSION)
  }
})

test('cargo takes the archives from the checked directory', () => {
  const config = read('.cargo/config.toml')
  assert.match(
    config,
    new RegExp(
      `SHERPA_ONNX_ARCHIVE_DIR = \\{ value = "${ARCHIVE_DIR}", relative = true \\}`,
    ),
  )
})

test('every pinned archive has a SHA-256 and belongs to the pinned version', () => {
  for (const target of knownTargets()) {
    const archive = archiveFor(target)
    assert.match(archive.sha256, /^[0-9a-f]{64}$/, target)
    assert.ok(archive.size > 0, target)
    assert.ok(archive.name.includes(`v${SHERPA_ONNX_VERSION}-`), target)
  }
})

test('every target holzi builds has an archive', () => {
  for (const target of [
    'x86_64-unknown-linux-gnu',
    'x86_64-pc-windows-msvc',
    'aarch64-apple-darwin',
    'x86_64-apple-darwin',
    'aarch64-linux-android',
    'x86_64-linux-android',
  ]) {
    assert.doesNotThrow(() => archiveFor(target), target)
  }
  assert.throws(() => archiveFor('riscv64gc-unknown-linux-gnu'))
})

test('desktop links statically, mobile takes the shared builds', () => {
  assert.match(archiveFor('x86_64-unknown-linux-gnu').name, /-static-lib\./)
  assert.match(archiveFor('x86_64-pc-windows-msvc').name, /-static-MT-/)
  assert.match(archiveFor('aarch64-linux-android').name, /-android\.tar/)
  assert.match(
    archiveFor('aarch64-apple-ios').name,
    /ios-shared-onnxruntime-static/,
  )
})

test('download URLs follow the release layout of sherpa-onnx-sys', () => {
  assert.equal(
    downloadUrl(archiveFor('x86_64-unknown-linux-gnu')),
    `https://github.com/k2-fsa/sherpa-onnx/releases/download/v${SHERPA_ONNX_VERSION}/sherpa-onnx-v${SHERPA_ONNX_VERSION}-linux-x64-static-lib.tar.bz2`,
  )
  assert.match(
    downloadUrl(archiveFor('aarch64-apple-ios')),
    /\/releases\/download\/xcframework\//,
  )
})

test('the host target maps node platforms to Rust triples', () => {
  assert.equal(hostTarget('linux', 'x64'), 'x86_64-unknown-linux-gnu')
  assert.equal(hostTarget('darwin', 'arm64'), 'aarch64-apple-darwin')
  assert.equal(hostTarget('win32', 'x64'), 'x86_64-pc-windows-msvc')
  assert.throws(() => hostTarget('freebsd', 'x64'))
})
