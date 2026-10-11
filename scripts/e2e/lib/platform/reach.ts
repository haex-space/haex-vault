// Makes a service of the run (Nostr test relay, stand-in provider, RustFS, extension dev server)
// reachable from the device under test at the same address, `127.0.0.1:<port>` (spec 043, research
// R14): elsewhere the address is already the same machine. Every device then uses the same URLs, which
// matters for settings a vault syncs between devices.
//
// On an emulator the device's own firewall sends `127.0.0.1:<port>` on to this machine through the
// emulator's network (`10.0.2.2`). `adb reverse` would carry the same traffic through the one adb
// connection the driver also uses, and a large sync over it made adb drop the emulator
// ("device offline", the web view's DevTools gone with it). A phone without `su` keeps `adb reverse`.
import { execFile } from 'node:child_process'
import { createServer } from 'node:net'
import type { AddressInfo } from 'node:net'
import { promisify } from 'node:util'
import { createAdb, realAdbRunner } from './adb.ts'
import type { Adb, AdbRunner } from './adb.ts'

/** This machine as the emulator sees it (its virtual router). */
export const EMULATOR_HOST = '10.0.2.2'
/** Set by the Android preflight when the device redirects (`prepareRedirect`). */
export const REDIRECT_ENV = 'E2E_ANDROID_REDIRECT'

const CHAIN_OUT = 'holzi_e2e_out'
const CHAIN_POST = 'holzi_e2e_post'

/**
 * Prepares the device's firewall for `reachFromDevice`, once per run: two chains of its own, emptied,
 * and loopback traffic allowed to leave. False when the device has no `su` (a phone).
 */
export function prepareRedirect(adb: Adb): boolean {
  try {
    if (adb.su('id -u') !== '0') return false
  } catch {
    return false
  }
  adb.su(
    [
      'set -e',
      'sysctl -w net.ipv4.conf.all.route_localnet=1 >/dev/null',
      `iptables -t nat -N ${CHAIN_OUT} 2>/dev/null || true`,
      `iptables -t nat -F ${CHAIN_OUT}`,
      `iptables -t nat -C OUTPUT -j ${CHAIN_OUT} 2>/dev/null || iptables -t nat -A OUTPUT -j ${CHAIN_OUT}`,
      `iptables -t nat -N ${CHAIN_POST} 2>/dev/null || true`,
      `iptables -t nat -F ${CHAIN_POST}`,
      `iptables -t nat -C POSTROUTING -j ${CHAIN_POST} 2>/dev/null || iptables -t nat -A POSTROUTING -j ${CHAIN_POST}`,
      // The answers come back to the emulator's own address, not to a loopback one.
      `iptables -t nat -A ${CHAIN_POST} -p tcp -s 127.0.0.1 -d ${EMULATOR_HOST} -j MASQUERADE`,
    ].join('\n'),
  )
  return true
}

export function reachFromDevice(
  port: number,
  env: NodeJS.ProcessEnv = process.env,
  run: AdbRunner = realAdbRunner,
): void {
  const serial = env.ANDROID_SERIAL
  if (env.E2E_PLATFORM !== 'android' || serial === undefined || serial === '')
    return
  const adb = createAdb(serial, run)
  if (env[REDIRECT_ENV] === '1') {
    adb.su(
      `iptables -t nat -A ${CHAIN_OUT} -p tcp -d 127.0.0.1 --dport ${port} -j DNAT --to-destination ${EMULATOR_HOST}:${port}`,
    )
  } else {
    adb.reverse(port)
  }
}

/** Runs `adb -s <serial> shell <command>` without blocking, so a server of this process can answer. */
async function shellAsync(serial: string, command: string): Promise<string> {
  const { stdout } = await promisify(execFile)(
    'adb',
    ['-s', serial, 'shell', command],
    { timeout: 10_000 },
  )
  return stdout
}

const REACH_MARK = 'holzi-e2e-reach'

/**
 * Whether the device reaches this machine through the redirect: a throwaway server here answers, and
 * the device connects to it at `127.0.0.1` as the shell user. Tried for up to `limitMs`, because the
 * emulator's network can come up late, or not at all (one CI emulator never got a route to
 * `10.0.2.2`, and every service of its shard was out of reach).
 */
export async function redirectWorks(
  serial: string,
  limitMs = 30_000,
  shell: (command: string) => Promise<string> = (command) =>
    shellAsync(serial, command),
  run: AdbRunner = realAdbRunner,
): Promise<boolean> {
  const server = createServer((socket) => socket.end(`${REACH_MARK}\n`))
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject)
    server.listen(0, '127.0.0.1', resolve)
  })
  try {
    const { port } = server.address() as AddressInfo
    reachFromDevice(
      port,
      { E2E_PLATFORM: 'android', ANDROID_SERIAL: serial, [REDIRECT_ENV]: '1' },
      run,
    )
    const end = Date.now() + limitMs
    for (;;) {
      try {
        const answer = await shell(`nc -w 2 127.0.0.1 ${port} </dev/null`)
        if (answer.includes(REACH_MARK)) return true
      } catch {
        // Refused or timed out: not (yet) reachable.
      }
      if (Date.now() >= end) return false
      await new Promise((resolve) => setTimeout(resolve, 1000))
    }
  } finally {
    server.close()
  }
}
