/**
 * Runs holzi built with the feature `platform-probe` and judges its one result line (spec 044,
 * T007, research R4). Used by the CI jobs `platform-probe` on Windows and macOS:
 *
 *   node scripts/run-platform-probe.ts <path to the holzi binary>
 *
 * and by `platform-probe-android`, which judges a saved logcat, optionally with one step that must
 * be there with a detail starting so:
 *
 *   node scripts/run-platform-probe.ts --judge <logcat file> [--expect <step>=<detail start>]
 *
 * Exit 0 only when the line reports `ok: true` and the process ended with 0.
 */
import { spawn } from 'node:child_process'
import { readFileSync } from 'node:fs'

import {
  judgeProbe,
  parseExpectation,
  PROBE_TIMEOUT_MS,
} from './lib/platformProbeResult.ts'

if (process.argv[2] === '--judge') {
  const file = process.argv[3]
  if (!file) {
    console.error(
      'usage: node scripts/run-platform-probe.ts --judge <logcat file> [--expect <step>=<detail start>]',
    )
    process.exit(2)
  }
  const expect =
    process.argv[4] === '--expect'
      ? parseExpectation(process.argv[5])
      : undefined
  if (process.argv[4] !== undefined && expect === undefined) {
    console.error(`unknown option: ${process.argv.slice(4).join(' ')}`)
    process.exit(2)
  }
  const verdict = judgeProbe(readFileSync(file, 'utf8'), null, expect)
  console.log(verdict.message)
  process.exit(verdict.ok ? 0 : 1)
}

const binary = process.argv[2]
if (!binary) {
  console.error('usage: node scripts/run-platform-probe.ts <holzi binary>')
  process.exit(2)
}

const child = spawn(binary, [], {
  env: { ...process.env, HOLZI_PROBE: '1' },
  stdio: ['ignore', 'pipe', 'pipe'],
})
let output = ''
child.stdout.on('data', (chunk: Buffer) => {
  output += chunk.toString()
  process.stdout.write(chunk)
})
child.stderr.on('data', (chunk: Buffer) => process.stderr.write(chunk))

const timer = setTimeout(() => {
  console.error(`platform probe: no exit within ${PROBE_TIMEOUT_MS / 1000} s`)
  child.kill()
}, PROBE_TIMEOUT_MS)

child.on('exit', (code) => {
  clearTimeout(timer)
  const verdict = judgeProbe(output, code)
  console.log(verdict.message)
  process.exit(verdict.ok ? 0 : 1)
})
