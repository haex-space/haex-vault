import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, it } from 'node:test'
import { pidAlive } from '../processes.ts'
import type { DataHandle } from './host.ts'
import { LinuxData, createLinuxHost } from './linux.ts'
import type { E2EEnv } from '../scenario-types.ts'
import type { Instance } from '../instance.ts'

const VAULT_DIR = ['data', 'com.haex.holzi', 'vaults']
/** The folder of the vault `v` the tests copy. */
const FOLDER = [...VAULT_DIR, 'v']

let dir: string
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'e2e-linux-host-'))
})
afterEach(() => rmSync(dir, { recursive: true, force: true }))

function dataWith(name: string, files: Record<string, string>): LinuxData {
  const data = new LinuxData(join(dir, name))
  const target = join(data.root, ...FOLDER)
  mkdirSync(target, { recursive: true })
  for (const [file, content] of Object.entries(files)) {
    writeFileSync(join(target, file), content)
  }
  return data
}

const listed = (data: LinuxData) => {
  const folder = join(data.root, ...FOLDER)
  return existsSync(folder) ? readdirSync(folder).sort() : []
}

describe('copyVaultFile', () => {
  it('copies the file and its journal files, never the lock file', async () => {
    const source = dataWith('a', {
      'vault.db': 'main',
      'vault.db-wal': 'journal',
      'vault.db-shm': 'shared',
      'vault.db.lock': 'lock',
      'notes.txt': 'not part of the database',
    })
    const target = new LinuxData(join(dir, 'b'))
    await source.copyVaultFile('v', target)
    assert.deepEqual(listed(target), [
      'vault.db',
      'vault.db-shm',
      'vault.db-wal',
    ])
    assert.equal(
      readFileSync(join(target.root, ...FOLDER, 'vault.db-wal'), 'utf8'),
      'journal',
    )
  })

  it('refuses a source that still runs', async () => {
    const source = dataWith('a', { 'vault.db': 'main' })
    source.running = true
    await assert.rejects(
      source.copyVaultFile('v', new LinuxData(join(dir, 'b'))),
      /still running/,
    )
    assert.equal(existsSync(join(dir, 'b')), false)
  })

  it('reports a missing vault file', async () => {
    const source = dataWith('a', { 'notes.txt': 'x' })
    await assert.rejects(
      source.copyVaultFile('v', new LinuxData(join(dir, 'b'))),
      /no vault file of "v"/,
    )
  })

  it('refuses a target from another host', async () => {
    const source = dataWith('a', { 'vault.db': 'main' })
    const foreign: DataHandle = {
      copyVaultFile: async () => {},
      keep: () => {},
      dispose: () => {},
    }
    await assert.rejects(source.copyVaultFile('v', foreign), /same host/)
  })

  it('leaves nothing partial in the target when a file cannot be copied', async () => {
    const source = dataWith('a', { 'vault.db': 'main' })
    // A directory where the journal should be makes the second copy fail after the first succeeded.
    mkdirSync(join(source.root, ...FOLDER, 'vault.db-wal'))
    const target = new LinuxData(join(dir, 'b'))
    await assert.rejects(
      source.copyVaultFile('v', target),
      /copying the vault file/,
    )
    assert.deepEqual(listed(target), [])
    assert.equal(existsSync(join(target.root, ...FOLDER)), false)
  })

  it('does not copy into a target it cannot write', async () => {
    const source = dataWith('a', { 'vault.db': 'main' })
    const target = dataWith('b', {})
    chmodSync(join(target.root, ...FOLDER), 0o500)
    try {
      await assert.rejects(
        source.copyVaultFile('v', target),
        /copying the vault file/,
      )
    } finally {
      chmodSync(join(target.root, ...FOLDER), 0o700)
    }
    assert.deepEqual(listed(target), [])
  })
})

describe('keep and dispose', () => {
  it('keep copies the data into a folder and dispose removes it', () => {
    const data = dataWith('a', { 'vault.db': 'main' })
    data.keep(join(dir, 'material'))
    assert.equal(
      readFileSync(
        join(dir, 'material', 'data', ...FOLDER, 'vault.db'),
        'utf8',
      ),
      'main',
    )
    data.dispose()
    assert.equal(existsSync(data.root), false)
  })

  it('keep of data that never existed does nothing', () => {
    new LinuxData(join(dir, 'none')).keep(join(dir, 'material'))
    assert.equal(existsSync(join(dir, 'material')), false)
  })
})

describe('the host', () => {
  const env = {
    runDir: '',
    app: '/bin/true',
    tools: {},
    marker: 'm',
  } as unknown as E2EEnv

  it('keeps data and logs below the run directory, by device folder', () => {
    const host = createLinuxHost({
      env: { ...env, runDir: join(dir, 'run') },
      scenario: 'sc',
    })
    const data = host.newData('4-anna-6-laptop')
    assert.ok(data instanceof LinuxData)
    assert.equal(data.root, join(dir, 'run', 'instances', 'sc-4-anna-6-laptop'))
  })

  it('kill ends the whole group without the shutdown and frees the data for a copy', async () => {
    const child = spawn('sleep', ['60'], { detached: true, stdio: 'ignore' })
    const pid = child.pid as number
    const fake = {
      driverPid: pid,
      pid,
      alive: () => pidAlive(pid),
      stop: async () => {},
      screenshot: async () => Buffer.alloc(0),
    } as unknown as Instance
    const host = createLinuxHost({
      env: { ...env, runDir: join(dir, 'run') },
      scenario: 'sc',
      startInstance: async () => fake,
    })
    const data = host.newData('d')
    const device = await host.start({ data, folder: 'd' })
    assert.equal((data as LinuxData).running, true)
    await assert.rejects(host.start({ data, folder: 'd' }), /already runs/)
    await device.kill()
    assert.equal(pidAlive(pid), false)
    assert.equal((data as LinuxData).running, false)
  })
})
