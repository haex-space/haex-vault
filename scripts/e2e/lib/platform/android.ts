// The Android implementation of the driver layer (spec 043, contract e2e-android.md): the app on one
// device (an emulator in CI), started and ended through adb, driven through chromedriver attached to
// its web view. One device holds one app's data, so a scenario has one Android device at most; the
// others of a group run on Linux (`group.ts`); the address book's controls: `android-contacts.ts`.
import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { basename, join } from 'node:path'
import type { ColorScheme, Instance, PhoneControls } from '../instance.ts'
import { newSessionWithRetry } from '../instance.ts'
import { createPage } from '../page.ts'
import type { StepRecorder } from '../page.ts'
import { WebDriverClient } from '../webdriver.ts'
import { withDevice } from './adb.ts'
import { contactControls } from './android-contacts.ts'
import type { Adb } from './adb.ts'
import { androidCapabilities, startChromedriver } from './chromedriver.ts'
import type { Chromedriver } from './chromedriver.ts'
import type { DataHandle, DeviceHost, RunningDevice } from './host.ts'
import { LinuxData, VAULT_DIR as LINUX_VAULT_DIR } from './linux.ts'

export const PACKAGE = 'com.haex.holzi'
const ACTIVITY = `${PACKAGE}/.MainActivity`
/** Where the app keeps its vault files, relative to its data directory (`get_app_local_data`). */
const VAULT_DIR = 'instances'

const sleep = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms))

/**
 * The app's pages on Android are served from `http://tauri.localhost` and extension frames from
 * `http://holzi-ext.localhost` (Tauri's custom protocols on Android); scenarios name the desktop URLs.
 */
export function mapAndroidUrl(url: string): string {
  return url
    .replace(/^tauri:\/\/localhost/, 'http://tauri.localhost')
    .replace(/^holzi-ext:\/\/localhost/, 'http://holzi-ext.localhost')
}

/**
 * Brings the device into the state a scenario expects before the app starts: its data cleared unless
 * the scenario reuses it, "Zugriff auf alle Dateien" given as a person would (spec 044 FR-039)
 * unless the scenario starts without it, a desktop-sized display (about 1280×800 dp, like the Linux
 * virtual screen) or a phone-sized one (360×800 dp), and the requested colour scheme. The size is set
 * before the start: the web view does not take a new density over while it runs; the permission
 * too: taking it from a running app ends it.
 */
export function prepareDevice(
  adb: Adb,
  options: {
    reuse?: boolean
    colorScheme?: ColorScheme
    phoneScreen?: boolean
    withoutAllFiles?: boolean
  },
): void {
  if (options.reuse !== true) adb.shell('pm', 'clear', PACKAGE)
  adb.shell(
    'appops',
    'set',
    PACKAGE,
    ALL_FILES,
    options.withoutAllFiles === true ? 'default' : 'allow',
  )
  if (options.phoneScreen === true) {
    adb.shell('wm', 'size', '1080x2400')
    adb.shell('wm', 'density', '480')
  } else {
    // At density 160, not 320: the same 1280×800 dp in a quarter of the pixels. The emulator
    // renders on the CPU (SwiftShader); a film in a maximized window at 2560×1600 froze it.
    adb.shell('wm', 'size', '1280x800')
    adb.shell('wm', 'density', '160')
  }
  adb.shell(
    'cmd',
    'uimode',
    'night',
    options.colorScheme === 'dark' ? 'yes' : 'no',
  )
}

/** The phone's own controls for the Android scenarios (spec 043, contract e2e-android.md). */
export function phoneControls(adb: Adb): PhoneControls {
  return {
    ...contactControls(adb),
    back: () => void adb.shell('input', 'keyevent', 'KEYCODE_BACK'),
    // As the system does under memory pressure: no shutdown of the app's own. `am kill` does not
    // end a process that has only just gone to the background, so the app's own user ends it.
    killInBackground: () => {
      adb.shell('input', 'keyevent', 'KEYCODE_HOME')
      const pid = adb.pidof(PACKAGE)
      if (pid !== undefined) adb.runAs(PACKAGE, `kill -9 ${pid}`)
    },
    screenProtected: () => windowFlags(adb).includes('SECURE'),
    // Granting does not end the app; a refusal only marks the permission as decided, since revoking
    // a granted one would end the process.
    allowNotifications: (allowed) => {
      if (allowed) {
        adb.shell(
          'pm',
          'clear-permission-flags',
          PACKAGE,
          NOTIFICATIONS,
          'user-fixed',
        )
        adb.shell('pm', 'grant', PACKAGE, NOTIFICATIONS)
      } else {
        adb.shell(
          'pm',
          'set-permission-flags',
          PACKAGE,
          NOTIFICATIONS,
          'user-set',
          'user-fixed',
        )
      }
    },
    notificationTitles: () =>
      notificationTitles(adb.shell('dumpsys', 'notification', '--noredact')),
    crashLines: (pid) =>
      crashLines(adb.shell('logcat', '-d', '-b', 'crash', `--pid=${pid}`)),
    // Granting does not end the app (taking it would); the person's way back is the back key.
    allowAllFiles: () => {
      adb.shell('appops', 'set', PACKAGE, ALL_FILES, 'allow')
    },
    frontPackage: () =>
      frontPackage(adb.shell('dumpsys', 'activity', 'activities')),
    frontDetail: () =>
      adb
        .shell('dumpsys', 'activity', 'activities')
        .split('\n')
        .filter((line) => /ResumedActivity/.test(line))
        .map((line) => line.trim())
        .join(' | '),
    // The task exists: the single-task activity comes to the front without a new process.
    bringToFront: () => void adb.shell('am', 'start', '-n', ACTIVITY),
    // Not granted until now, so marking it decided does not end the app.
    refuseMicrophone: () => {
      adb.shell(
        'pm',
        'set-permission-flags',
        PACKAGE,
        MICROPHONE,
        'user-set',
        'user-fixed',
      )
    },
  }
}

const NOTIFICATIONS = 'android.permission.POST_NOTIFICATIONS'
const MICROPHONE = 'android.permission.RECORD_AUDIO'
const ALL_FILES = 'MANAGE_EXTERNAL_STORAGE' // the app op of "Zugriff auf alle Dateien" (API 30 on)

/** The package of the resumed activity in the output of `dumpsys activity activities`. */
export function frontPackage(dump: string): string | undefined {
  return /ResumedActivity:? ActivityRecord\{\S+ \S+ ([\w.]+)\//.exec(dump)?.[1]
}

/** The entries of `logcat -b crash` output, without its section headers. */
export function crashLines(dump: string): string[] {
  return dump
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '' && !line.startsWith('---------'))
}

/** The titles of the app's notifications in the output of `dumpsys notification --noredact`. */
export function notificationTitles(dump: string): string[] {
  return dump
    .split('NotificationRecord(')
    .filter((record) => record.includes(`pkg=${PACKAGE}`))
    .flatMap((record) => {
      const title = /android\.title=String \((.*)\)/.exec(record)
      return title === null ? [] : [title[1]!]
    })
}

/**
 * What closing the window is on a phone: the person swipes the app away among the recent apps, which
 * removes its task (`cmd activity stack remove`).
 */
export function removeTask(adb: Adb): void {
  const list = adb.shell('cmd', 'activity', 'stack', 'list')
  const task = new RegExp(
    `taskId=(\\d+): ${PACKAGE.replaceAll('.', '\\.')}/`,
  ).exec(list)?.[1]
  if (task === undefined) throw new Error(`no task of ${PACKAGE} to remove`)
  adb.shell('cmd', 'activity', 'stack', 'remove', task)
}

/** The flags of the app's window, as `dumpsys window` lists them (`fl=… SECURE …`). */
function windowFlags(adb: Adb): string {
  const dump = adb.shell('dumpsys', 'window', 'windows')
  const start = dump.search(new RegExp(`Window\\{[^}]*${PACKAGE}`))
  if (start === -1) return ''
  const rest = dump.slice(start)
  const next = rest.slice(1).search(/\n\s*Window #/)
  const block = next === -1 ? rest : rest.slice(0, next + 1)
  return block.match(/fl=[^\n]*/)?.[0] ?? ''
}

/** Puts the device's display back the way it was. */
export function restoreDevice(adb: Adb): void {
  adb.shell('wm', 'size', 'reset')
  adb.shell('wm', 'density', 'reset')
}

/** Starts the app and waits until its web view accepts a debugger; resolves with its pid. */
export async function launchApp(adb: Adb, limitMs = 20_000): Promise<number> {
  adb.shell('am', 'start', '-W', '-n', ACTIVITY)
  const end = Date.now() + limitMs
  for (;;) {
    const pid = adb.pidof(PACKAGE)
    if (pid !== undefined && adb.devtoolsOpen(pid)) return pid
    if (Date.now() >= end) {
      throw new Error(
        `the app's web view did not open for debugging within ${limitMs} ms (is the APK a debug build?)`,
      )
    }
    await sleep(100)
  }
}

export interface AndroidStartOptions {
  adb: Adb
  /** The chromedriver for the device's web view version. */
  chromedriver: string
  marker: string
  logFile: string
  colorScheme?: ColorScheme
  /** The app's data stays from the instance before. */
  reuse?: boolean
  /** A phone-sized screen instead of the desktop-sized one. */
  phoneScreen?: boolean
  /** Without "Zugriff auf alle Dateien". */
  withoutAllFiles?: boolean
  step?: StepRecorder
}

/** The app on the device as an `Instance` (the same surface a Linux instance offers). */
export async function startAndroidInstance(
  options: AndroidStartOptions,
): Promise<Instance> {
  const { adb } = options
  prepareDevice(adb, options)
  const pid = await launchApp(adb)
  const forceStop = () => {
    try {
      withDevice(adb, () => adb.shell('am', 'force-stop', PACKAGE))
    } catch {
      // The device is gone for good; there is nothing left to end then.
    }
  }
  let driver: Chromedriver
  try {
    driver = await startChromedriver({
      binary: options.chromedriver,
      marker: options.marker,
      logFile: options.logFile,
    })
  } catch (error) {
    forceStop()
    throw error
  }
  const client = new WebDriverClient(`http://127.0.0.1:${driver.port}`)
  const stop = async () => {
    try {
      // Frees the device for the next session; chromedriver refuses a second one otherwise.
      await client.deleteSession()
    } catch {
      // The app may already have ended.
    }
    try {
      await driver.stop()
    } finally {
      forceStop()
      restoreDevice(adb)
    }
  }
  try {
    await newSessionWithRetry(client, androidCapabilities(PACKAGE, adb.serial))
  } catch (error) {
    await stop()
    throw new Error(
      `could not attach to the app on ${adb.serial}: ${(error as Error).message} (see ${options.logFile})`,
      { cause: error },
    )
  }
  const alive = () => adb.pidof(PACKAGE) === pid
  const step: StepRecorder = options.step ?? (() => {})
  const page = createPage({
    client,
    step,
    process: {
      alive,
      marked: () => {
        const running = adb.pidof(PACKAGE)
        return running === undefined
          ? []
          : [{ pid: running, marker: options.marker, exe: PACKAGE }]
      },
    },
    mapUrl: mapAndroidUrl,
    replaceOnNavigate: true,
    closeWindow: () => Promise.resolve(removeTask(adb)),
  })
  return {
    ...page,
    root: `android:${adb.serial}`,
    marker: options.marker,
    logFile: options.logFile,
    ports: { driver: driver.port, native: 0 },
    driverPid: driver.pid,
    pid,
    screenshot: (callLimitMs) => client.screenshot(callLimitMs),
    alive,
    stop,
    step,
    phone: phoneControls(adb),
  }
}

/** Writes `<what>-note.txt` beside the material that could not be kept. */
function keptNote(folder: string, what: string, error: unknown): void {
  try {
    mkdirSync(folder, { recursive: true })
    writeFileSync(
      join(folder, `${what}-note.txt`),
      `not kept: ${error instanceof Error ? error.message : String(error)}\n`,
    )
  } catch {
    // Nothing more to say.
  }
}

/** The app's data on the device; the device holds one at a time. */
export class AndroidData implements DataHandle {
  readonly adb: Adb
  /** True while the app runs over this data. */
  running = false
  /** True once the app has run over it (the next start keeps the data). */
  used = false

  constructor(adb: Adb) {
    this.adb = adb
  }

  /**
   * Copies the vault file to a device on Linux, the only other host of a run with a phone (stage 2).
   * Read as the app through `run-as`; each file's size is checked against the phone's.
   */
  async copyVaultFile(vaultName: string, to: DataHandle): Promise<void> {
    if (!(to instanceof LinuxData)) {
      throw new Error(
        'a vault file on the phone can be copied to a Linux device only',
      )
    }
    if (this.running) {
      throw new Error(
        'the source device is still running; stop it before copying its vault file',
      )
    }
    let listing = ''
    try {
      listing = this.adb.runAs(PACKAGE, `ls ${VAULT_DIR}`).toString()
    } catch {
      // No folder: no vault file, said below.
    }
    // The file with what the closed app still keeps beside it; never the lock file.
    const files = listing
      .split(/\s+/)
      .filter(
        (file) => file.startsWith(`${vaultName}.db`) && !file.endsWith('.lock'),
      )
      .sort()
    if (!files.includes(`${vaultName}.db`)) {
      throw new Error(`there is no vault file of "${vaultName}" to copy`)
    }
    const target = join(to.root, ...LINUX_VAULT_DIR)
    mkdirSync(target, { recursive: true })
    const copied: string[] = []
    try {
      for (const file of files) {
        writeFileSync(
          join(target, file),
          this.readWhole(`${VAULT_DIR}/${file}`),
        )
        copied.push(file)
      }
    } catch (error) {
      for (const file of copied) rmSync(join(target, file), { force: true })
      throw new Error(
        `copying the vault file of "${vaultName}" failed: ${(error as Error).message}`,
        { cause: error },
      )
    }
  }

  /**
   * The file as the app sees it, read through `run-as`. adb can lose the emulator in the middle of
   * a transfer and hand back what came before (the size check catches that); reading changes
   * nothing, so it is read again once the device is back, up to three times.
   */
  private readWhole(path: string): Buffer {
    for (let attempt = 1; ; attempt++) {
      try {
        const bytes = this.adb.runAs(PACKAGE, `cat ${path}`)
        const size = Number(this.adb.runAs(PACKAGE, `stat -c %s ${path}`))
        if (bytes.length !== size) {
          throw new Error(`${basename(path)} was read only in part`)
        }
        return bytes
      } catch (error) {
        if (attempt === 3 || !this.adb.waitForDevice()) throw error
      }
    }
  }

  keep(folder: string): void {
    try {
      // The vaults and the app's log, as a Linux device keeps its whole data folder; not the caches.
      const tar = this.adb.runAs(
        PACKAGE,
        'tar c --exclude=./cache --exclude=./code_cache --exclude=./app_webview .',
      )
      mkdirSync(folder, { recursive: true })
      writeFileSync(join(folder, 'android-data.tar'), tar)
    } catch (error) {
      // Diagnostics must never hide the failure they describe, but say why they are missing.
      keptNote(folder, 'android-data', error)
    }
    try {
      // The app also logs to logcat, and its own log file sometimes stops early on the device.
      const logcat = this.adb.shell('logcat', '-d', '-v', 'time', '-t', '20000')
      mkdirSync(folder, { recursive: true })
      writeFileSync(join(folder, 'logcat.txt'), logcat)
    } catch (error) {
      keptNote(folder, 'logcat', error)
    }
  }

  dispose(): void {
    withDevice(this.adb, () => {
      try {
        restoreDevice(this.adb)
      } finally {
        this.adb.shell('pm', 'clear', PACKAGE)
      }
    })
  }
}

/**
 * Ends the app the way a person does on a phone: the vault closes, and the app with it (FR-006). Its
 * sync says goodbye to the other devices on the way; after a forced stop they would wait out the
 * connection's idle time behind the relay and meanwhile refuse the phone's new process as a
 * duplicate. A device without an open vault simply ends.
 */
export async function closeVault(
  instance: Pick<Instance, 'invoke' | 'pid'>,
  adb: Pick<Adb, 'pidof'>,
  limitMs = 10_000,
): Promise<void> {
  if (adb.pidof(PACKAGE) !== instance.pid) return
  // The app ends while the call is under way; a refusal (no vault open) comes at once.
  let refused = false
  void instance.invoke('close_instance', undefined, { expectEnd: true }).then(
    (result) => void (refused = 'ok' in result && !result.ok),
    () => {},
  )
  const end = Date.now() + limitMs
  while (!refused && adb.pidof(PACKAGE) === instance.pid && Date.now() < end) {
    await sleep(100)
  }
}

export interface AndroidHostOptions {
  adb: Adb
  chromedriver: string
  marker: string
  logDir: string
}

/** One Android device as a host; a second device in the same scenario is refused. */
export function createAndroidHost(options: AndroidHostOptions): DeviceHost {
  let data: AndroidData | undefined
  return {
    newData() {
      if (data !== undefined) {
        throw new Error(
          'a scenario can have one Android device; the others run on Linux',
        )
      }
      data = new AndroidData(options.adb)
      return data
    },
    async start(start): Promise<RunningDevice> {
      const own = start.data
      if (!(own instanceof AndroidData)) {
        throw new Error('the data of a device must come from the same host')
      }
      if (own.running) throw new Error('the app already runs on the device')
      const instance = await startAndroidInstance({
        adb: options.adb,
        chromedriver: options.chromedriver,
        marker: options.marker,
        logFile: join(options.logDir, start.folder, 'driver.log'),
        reuse: own.used,
        step: start.step,
      })
      own.running = true
      own.used = true
      const kill = async () => {
        await instance.stop()
        own.running = false
      }
      const stop = async () => {
        try {
          await closeVault(instance, options.adb)
        } finally {
          // Ended either way: an app left running would hold the device for the next scenario.
          await kill()
        }
      }
      return { ...instance, stop, kill }
    },
  }
}
