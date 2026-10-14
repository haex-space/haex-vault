// The calls the Android platform makes to one device through adb (spec 043, contract e2e-android.md).
// One function runs every command, so a check can replace it and see the exact argument lists.
import { execFileSync } from 'node:child_process'

/** Runs `adb <args>` and returns its standard output; throws when adb fails. */
export type AdbRunner = (args: string[], input?: Buffer) => Buffer

export const realAdbRunner: AdbRunner = (args, input) =>
  execFileSync('adb', args, {
    input,
    maxBuffer: 1024 * 1024 * 1024,
    stdio: ['pipe', 'pipe', 'pipe'],
  })

export interface Adb {
  readonly serial: string
  /** `adb shell <args>`, trimmed text. */
  shell(...args: string[]): string
  /** The app's process id, or undefined when it does not run. Synchronous: `alive()` needs it. */
  pidof(pkg: string): number | undefined
  /** Whether the app's web view accepts a debugger (its DevTools socket is open). */
  devtoolsOpen(pid: number): boolean
  /** Makes `port` on this machine reachable from the device as `127.0.0.1:port`. */
  reverse(port: number): void
  /** Runs `sh -c script` as the app (debug builds only) and returns its raw output. */
  runAs(pkg: string, script: string): Buffer
  /**
   * Runs `sh -c script` as root through the device's `su` (an emulator image with Google APIs lets the
   * shell use it, a phone does not) and returns its trimmed output. adbd itself stays the shell user,
   * so the files it pushes keep an owner the app may open (under `adb root` they do not).
   */
  su(script: string): string
  /** Copies a file of this machine to `remote` on the device (outside any app's storage). */
  push(local: string, remote: string): void
  /** Copies the file `remote` of the device (outside any app's storage) to this machine. */
  pull(remote: string, local: string): void
  /**
   * Waits until adb sees the device again (after its connection dropped, "device offline"); false
   * when it does not within `limitMs`. Synchronous, as the teardown that needs it.
   */
  waitForDevice(limitMs?: number): boolean
}

/** Blocks this thread for `ms` (the adb calls here are synchronous anyway). */
const sleepSync = (ms: number) =>
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms)

/**
 * Runs `action`; when adb fails, waits for the device to come back once and runs it again. A short
 * drop of the adb connection must not leave the app running for the next scenario.
 */
export function withDevice<T>(adb: Adb, action: () => T): T {
  try {
    return action()
  } catch (error) {
    if (!adb.waitForDevice()) throw error
    return action()
  }
}

/**
 * adb refused the command because it does not see the device right now: the command never reached
 * it, so running it again once the device is back is safe.
 */
export function deviceAway(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error)
  return /device offline|device '[^']*' not found|no devices\/emulators found/.test(
    message,
  )
}

export function createAdb(serial: string, run: AdbRunner = realAdbRunner): Adb {
  const once = (args: string[], input?: Buffer) =>
    run(['-s', serial, ...args], input)
  const waitForDevice = (limitMs = 30_000) => {
    const end = Date.now() + limitMs
    for (;;) {
      try {
        if (once(['get-state']).toString().trim() === 'device') return true
      } catch {
        // Offline or gone for now.
      }
      if (Date.now() >= end) return false
      sleepSync(500)
    }
  }
  // The emulator's adb connection drops now and then (seen right after the app ended and while a
  // large sync starts); a command refused for that runs again once the device is back.
  const adb = (args: string[], input?: Buffer) => {
    try {
      return once(args, input)
    } catch (error) {
      if (!deviceAway(error) || !waitForDevice()) throw error
      return once(args, input)
    }
  }
  return {
    serial,
    shell: (...args) =>
      adb(['shell', ...args])
        .toString()
        .trim(),
    pidof(pkg) {
      let out: string
      try {
        out = adb(['shell', 'pidof', pkg]).toString().trim()
      } catch {
        // pidof exits with 1 when nothing matches.
        return undefined
      }
      const pid = Number(out.split(/\s+/)[0])
      return Number.isInteger(pid) && pid > 0 ? pid : undefined
    },
    devtoolsOpen(pid) {
      return adb(['shell', 'cat', '/proc/net/unix'])
        .toString()
        .includes(`webview_devtools_remote_${pid}`)
    },
    reverse(port) {
      adb(['reverse', `tcp:${port}`, `tcp:${port}`])
    },
    runAs(pkg, script) {
      return adb(['exec-out', 'run-as', pkg, 'sh', '-c', script])
    },
    su(script) {
      // adb joins the arguments into one command line for the device's shell: quote the script.
      const quoted = `'${script.replaceAll("'", "'\\''")}'`
      return adb(['shell', 'su', '0', 'sh', '-c', quoted]).toString().trim()
    },
    push(local, remote) {
      adb(['push', local, remote])
    },
    // A pull only reads, so it runs again after any failure once the device is there: a connection
    // that drops during the transfer fails without "device offline" (files-encrypted-device on CI,
    // the adb server logging "connection terminated" 50 ms into the pull).
    pull(remote, local) {
      try {
        adb(['pull', remote, local])
      } catch (error) {
        if (!waitForDevice()) throw error
        once(['pull', remote, local])
      }
    },
    waitForDevice,
  }
}

/** The serial of the one device adb sees, or the one `ANDROID_SERIAL` names. */
export function deviceSerial(
  env: NodeJS.ProcessEnv,
  run: AdbRunner = realAdbRunner,
): string {
  if (env.ANDROID_SERIAL !== undefined && env.ANDROID_SERIAL !== '')
    return env.ANDROID_SERIAL
  const devices = run(['devices'])
    .toString()
    .split('\n')
    .slice(1)
    .map((line) => line.trim().split(/\s+/))
    .filter(([serial, state]) => serial !== undefined && state === 'device')
    .map(([serial]) => serial!)
  if (devices.length !== 1) {
    throw new Error(
      devices.length === 0
        ? 'adb sees no device; start an emulator or connect a phone'
        : `adb sees ${devices.length} devices; name one with ANDROID_SERIAL`,
    )
  }
  return devices[0]!
}
