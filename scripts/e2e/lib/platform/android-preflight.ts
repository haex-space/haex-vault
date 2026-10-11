// The checks before an Android run (spec 043, contract e2e-android.md): one device, the debug app on
// it (installed from `--apk` when given), a chromedriver whose major version is the device's web
// view's, and the iroh relay the devices of a group meet through. A mismatched chromedriver fails in
// ways that look like application bugs, so it stops the run.
import { execFileSync } from 'node:child_process'
import { delimiter, join } from 'node:path'
import { existsSync } from 'node:fs'
import { createAdb, deviceSerial, realAdbRunner } from './adb.ts'
import type { AdbRunner } from './adb.ts'
import { PACKAGE } from './android.ts'
import { prepareRedirect, REDIRECT_ENV } from './reach.ts'

export interface AndroidPreflight {
  ok: boolean
  messages: string[]
  versions: { driver?: string; webview?: string }
  /** What the scenarios need to reach the device: ANDROID_SERIAL, E2E_CHROMEDRIVER, E2E_IROH_RELAY. */
  env: Record<string, string>
}

export interface AndroidPreflightDeps {
  run: AdbRunner
  /** `<binary> --version`, or undefined when it cannot run. */
  version(binary: string): string | undefined
  exists(path: string): boolean
}

export const realAndroidPreflightDeps: AndroidPreflightDeps = {
  run: realAdbRunner,
  version(binary) {
    try {
      return execFileSync(binary, ['--version']).toString().trim()
    } catch {
      return undefined
    }
  },
  exists: existsSync,
}

/** The first number of a dotted version anywhere in `text` (`ChromeDriver 124.0.6367.207 (…)` → 124). */
export function majorOf(text: string | undefined): number | undefined {
  const match = text === undefined ? null : /(\d+)\.\d+\.\d+/.exec(text)
  return match === null ? undefined : Number(match[1])
}

function findOnPath(
  name: string,
  pathEnv: string | undefined,
  exists: (path: string) => boolean,
): string | undefined {
  for (const dir of (pathEnv ?? '').split(delimiter)) {
    const candidate = join(dir, name)
    if (dir !== '' && exists(candidate)) return candidate
  }
  return undefined
}

export function checkAndroidPreflight(
  options: { env: NodeJS.ProcessEnv; apk?: string },
  deps: AndroidPreflightDeps = realAndroidPreflightDeps,
): AndroidPreflight {
  const messages: string[] = []
  const versions: AndroidPreflight['versions'] = {}
  let serial: string
  try {
    serial = deviceSerial(options.env, deps.run)
  } catch (error) {
    return {
      ok: false,
      messages: [(error as Error).message],
      versions,
      env: {},
    }
  }
  const adb = createAdb(serial, deps.run)

  if (options.apk !== undefined) {
    if (!deps.exists(options.apk)) {
      messages.push(`--apk ${options.apk} does not exist`)
    } else {
      try {
        try {
          deps.run(['-s', serial, 'install', '-r', options.apk])
        } catch (error) {
          // An app signed with another key (a CI build over a local one) or with a higher version
          // (a release build) cannot be updated; the device only ever holds test data, so it is
          // replaced.
          const message = String((error as Error).message)
          if (
            !message.includes('INSTALL_FAILED_UPDATE_INCOMPATIBLE') &&
            !message.includes('INSTALL_FAILED_VERSION_DOWNGRADE')
          )
            throw error
          deps.run(['-s', serial, 'uninstall', PACKAGE])
          deps.run(['-s', serial, 'install', options.apk])
        }
      } catch (error) {
        messages.push(
          `installing ${options.apk} failed: ${(error as Error).message}`,
        )
      }
    }
  }
  let installed: string
  try {
    installed = adb.shell('pm', 'path', PACKAGE)
  } catch {
    installed = ''
  }
  if (!installed.includes('package:')) {
    messages.push(
      `${PACKAGE} is not installed on ${serial}; pass --apk with a debug APK (pnpm tauri android build --debug --apk --target x86_64)`,
    )
  } else {
    try {
      if (!adb.shell('dumpsys', 'package', PACKAGE).includes('DEBUGGABLE')) {
        messages.push(
          `${PACKAGE} on ${serial} is not a debug build; its web view cannot be driven`,
        )
      }
    } catch (error) {
      messages.push(
        `could not inspect ${PACKAGE} on ${serial}: ${(error as Error).message}`,
      )
    }
  }

  const chromedriver =
    options.env.E2E_CHROMEDRIVER !== undefined &&
    options.env.E2E_CHROMEDRIVER !== ''
      ? options.env.E2E_CHROMEDRIVER
      : findOnPath('chromedriver', options.env.PATH, deps.exists)
  if (chromedriver === undefined) {
    messages.push(
      'no chromedriver: set E2E_CHROMEDRIVER or put chromedriver on PATH (Chrome for Testing, same major version as the device web view)',
    )
  } else {
    versions.driver = deps.version(chromedriver)
    let webviewInfo: string | undefined
    try {
      webviewInfo = adb.shell(
        'dumpsys',
        'package',
        'com.google.android.webview',
      )
    } catch (error) {
      messages.push(
        `could not inspect com.google.android.webview on ${serial}: ${(error as Error).message}`,
      )
    }
    const webview = /versionName=(\S+)/.exec(webviewInfo ?? '')?.[1]
    versions.webview = webview
    const driverMajor = majorOf(versions.driver)
    const webviewMajor = majorOf(webview)
    if (driverMajor === undefined) {
      messages.push(`${chromedriver} --version did not answer`)
    } else if (webviewMajor !== undefined && driverMajor !== webviewMajor) {
      messages.push(
        `chromedriver ${driverMajor} does not match the device web view ${webviewMajor}; get chromedriver ${webviewMajor} from Chrome for Testing`,
      )
    }
  }

  // The phone is behind the emulator's network; the devices of a group reach it through this relay.
  const irohRelay =
    options.env.E2E_IROH_RELAY !== undefined &&
    options.env.E2E_IROH_RELAY !== ''
      ? options.env.E2E_IROH_RELAY
      : findOnPath('iroh-relay', options.env.PATH, deps.exists)
  if (irohRelay === undefined) {
    messages.push(
      'no iroh-relay: set E2E_IROH_RELAY or put iroh-relay on PATH (cargo install iroh-relay --version <iroh-relay in src-tauri/Cargo.lock> --features server --locked)',
    )
  } else if (deps.version(irohRelay) === undefined) {
    messages.push(`${irohRelay} --version did not answer`)
  }

  const env: Record<string, string> = { ANDROID_SERIAL: serial }
  try {
    // Services of the run reach an emulator through its network, not through adb (see `reach.ts`).
    if (prepareRedirect(adb)) env[REDIRECT_ENV] = '1'
  } catch (error) {
    messages.push(
      `could not prepare ${serial} to reach this machine: ${(error as Error).message}`,
    )
  }
  if (chromedriver !== undefined) env.E2E_CHROMEDRIVER = chromedriver
  if (irohRelay !== undefined) env.E2E_IROH_RELAY = irohRelay
  return { ok: messages.length === 0, messages, versions, env }
}
