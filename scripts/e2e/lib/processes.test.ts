import { after, describe, it } from 'node:test'
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import type { ChildProcess } from 'node:child_process'
import { once } from 'node:events'
import { mkdtempSync, readFileSync, readlinkSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  findByMarker,
  findByRoot,
  findOrphans,
  newMarker,
  parseMarker,
  pidAlive,
  processGroupOf,
  runnerStartTime,
  spawnMarked,
  stopGroup,
  stopLeftovers,
  stopRun,
  sweepOrphans,
} from './processes.ts'

const spawned: ChildProcess[] = []

/** A long sleep that carries the given marker, or none. */
function sleeper(
  marker?: string,
  detached = false,
): ChildProcess & { pid: number } {
  const env = { ...process.env }
  delete env.HOLZI_E2E_RUN
  if (marker) env.HOLZI_E2E_RUN = marker
  const child = spawn('sleep', ['60'], { env, stdio: 'ignore', detached })
  spawned.push(child)
  assert.ok(child.pid !== undefined)
  return child as ChildProcess & { pid: number }
}

async function waitGone(pid: number, limitMs = 3000): Promise<boolean> {
  const end = Date.now() + limitMs
  while (Date.now() < end) {
    if (!pidAlive(pid)) return true
    await new Promise((resolve) => setTimeout(resolve, 20))
  }
  return !pidAlive(pid)
}

after(() => {
  for (const child of spawned) {
    try {
      child.kill('SIGKILL')
    } catch {
      // already gone
    }
  }
})

describe(
  'processes',
  { skip: process.platform === 'linux' ? false : 'needs /proc (Linux only)' },
  () => {
    it('builds a marker from the runner pid, its start time and a random part, and parses it back', () => {
      const marker = newMarker()
      assert.match(marker, /^\d+:\d+:[0-9a-f]{16}$/)
      const parsed = parseMarker(marker)
      assert.equal(parsed?.runnerPid, process.pid)
      assert.equal(parsed?.runnerStart, runnerStartTime(process.pid))
      assert.equal(parseMarker('not a marker'), null)
    })

    it('finds a marked process and not an unmarked one', () => {
      const marker = newMarker()
      const marked = sleeper(marker)
      sleeper()
      assert.deepEqual(
        findByMarker(marker).map((p) => p.pid),
        [marked.pid],
      )
    })

    it('finds the application only by its executable', () => {
      const marker = newMarker()
      const child = sleeper(marker)
      const exe = readlinkSync(`/proc/${child.pid}/exe`)
      assert.deepEqual(
        findByMarker(marker, exe).map((p) => p.pid),
        [child.pid],
      )
      assert.deepEqual(findByMarker(marker, '/nonexistent/binary'), [])
    })

    it('treats a marker whose owner is gone as an orphan, and nothing else', async () => {
      const dead = spawn('true')
      await once(dead, 'exit')
      const liveRunner = sleeper()
      const liveStart = runnerStartTime(liveRunner.pid)
      const deadOwner = sleeper(`${dead.pid}:1:cccccccccccccccc`)
      const wrongStart = sleeper(`${liveRunner.pid}:1:bbbbbbbbbbbbbbbb`)
      const liveOther = sleeper(
        `${liveRunner.pid}:${liveStart}:aaaaaaaaaaaaaaaa`,
      )
      const ownRun = sleeper(newMarker())
      const unmarked = sleeper()
      const ours = new Set([
        deadOwner.pid,
        wrongStart.pid,
        liveOther.pid,
        ownRun.pid,
        unmarked.pid,
      ])

      const orphans = findOrphans()
        .map((p) => p.pid)
        .filter((pid) => ours.has(pid))
        .sort()
      assert.deepEqual(orphans, [deadOwner.pid, wrongStart.pid].sort())

      const swept = await sweepOrphans({ graceMs: 200 })
      assert.ok(swept.some((p) => p.pid === deadOwner.pid))
      assert.ok(await waitGone(deadOwner.pid))
      assert.ok(await waitGone(wrongStart.pid))
      for (const survivor of [liveRunner, liveOther, ownRun, unmarked]) {
        assert.ok(
          pidAlive(survivor.pid),
          `process ${survivor.pid} must survive the sweep`,
        )
      }
    })

    it('stops every process of a marker and leaves the rest', async () => {
      const marker = newMarker()
      const a = sleeper(marker)
      const b = sleeper(marker)
      const other = sleeper(newMarker())
      const unmarked = sleeper()
      const stopped = await stopRun(marker, { graceMs: 200 })
      assert.deepEqual(stopped.map((p) => p.pid).sort(), [a.pid, b.pid].sort())
      assert.ok(await waitGone(a.pid))
      assert.ok(await waitGone(b.pid))
      assert.ok(pidAlive(other.pid))
      assert.ok(pidAlive(unmarked.pid))
    })

    it('stops what an instance left running in its folder and nothing else', async () => {
      const root = mkdtempSync(join(tmpdir(), 'e2e-leftover-'))
      try {
        const inFolder = (folder: string) =>
          spawn('sleep', ['60'], {
            env: { ...process.env, XDG_CACHE_HOME: join(folder, 'cache') },
            stdio: 'ignore',
            detached: true,
          })
        const leftover = inFolder(root)
        const elsewhere = inFolder(join(root, 'other'))
        const unrelated = sleeper()
        spawned.push(leftover, elsewhere)
        assert.ok(leftover.pid !== undefined && elsewhere.pid !== undefined)
        assert.deepEqual(
          findByRoot(root).map((p) => p.pid),
          [leftover.pid],
        )
        const stopped = await stopLeftovers(root, { graceMs: 200 })
        assert.deepEqual(
          stopped.map((p) => p.pid),
          [leftover.pid],
        )
        assert.ok(await waitGone(leftover.pid))
        assert.ok(pidAlive(elsewhere.pid))
        assert.ok(pidAlive(unrelated.pid))
      } finally {
        rmSync(root, { recursive: true, force: true })
      }
    })

    it('stops a detached group including a grandchild', async () => {
      const shell = spawn('sh', ['-c', 'sleep 60 & echo $!; wait'], {
        detached: true,
        stdio: ['ignore', 'pipe', 'ignore'],
      })
      spawned.push(shell)
      assert.ok(shell.pid !== undefined)
      const [chunk] = (await once(shell.stdout!, 'data')) as [Buffer]
      const grandchild = Number(chunk.toString().trim())
      assert.ok(Number.isInteger(grandchild) && grandchild > 1)
      await stopGroup(shell.pid, { graceMs: 200 })
      assert.ok(await waitGone(shell.pid))
      assert.ok(await waitGone(grandchild))
    })

    it('ignores invalid process group ids', async () => {
      const child = sleeper(undefined, true)
      for (const pgid of [0, 1, -1, 1.5, Number.NaN]) {
        await stopGroup(pgid)
        assert.ok(pidAlive(child.pid), `group ${pgid} must not be signalled`)
      }
      await stopGroup(child.pid, { graceMs: 200 })
      assert.ok(await waitGone(child.pid))
    })

    it('spawns in its own group and logs each output line with a time and its stream', async () => {
      const dir = mkdtempSync(join(tmpdir(), 'e2e-processes-'))
      try {
        const logFile = join(dir, 'out.log')
        const marker = newMarker()
        const { child, closed } = spawnMarked(
          'sh',
          ['-c', 'echo hello; echo oops >&2'],
          {
            env: { ...process.env, HOLZI_E2E_RUN: marker },
            logFile,
          },
        )
        assert.ok(child.pid !== undefined)
        await closed
        const lines = readFileSync(logFile, 'utf8').trim().split('\n')
        assert.ok(
          lines.some((l) => /^\d{4}-\d\d-\d\dT[\d:.]+Z stdout hello$/.test(l)),
          lines.join('|'),
        )
        assert.ok(
          lines.some((l) => /^\d{4}-\d\d-\d\dT[\d:.]+Z stderr oops$/.test(l)),
          lines.join('|'),
        )
      } finally {
        rmSync(dir, { recursive: true, force: true })
      }
    })
  },
)

describe('processGroupOf', () => {
  it('is the pid of a detached child that leads its own group, and differs for an attached one', () => {
    const leader = sleeper(undefined, true)
    const member = sleeper()
    assert.equal(processGroupOf(leader.pid), leader.pid)
    assert.notEqual(processGroupOf(member.pid), member.pid)
  })

  it('is null for a process that is gone', async () => {
    const child = sleeper()
    child.kill('SIGKILL')
    await once(child, 'exit') // reaped: no entry of its own is left
    assert.equal(processGroupOf(child.pid), null)
  })
})
