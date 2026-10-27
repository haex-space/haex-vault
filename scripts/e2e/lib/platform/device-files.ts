// Files the app reads or writes during a scenario, in a folder the app can reach (spec 043, contract
// picked-file.md, test seam): on Linux a temporary folder of this machine, on Android a folder in the
// app's own download folder, written and read with `adb push` and `adb pull`. Chromedriver cannot
// work the system file picker, so scenarios hand the app such a path where a user would pick a file.
import {
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { basename, dirname, join, relative } from 'node:path'
import { randomBytes } from 'node:crypto'
import { createAdb } from './adb.ts'
import type { Adb } from './adb.ts'
import { PACKAGE } from './android.ts'

export interface DeviceFiles {
  folder: string
  path: (name: string) => string
  write: (name: string, content: string | Uint8Array) => void
  read: (name: string) => string
  readBytes: (name: string) => Buffer
  /** Empty files `sub/d<i>/f<i>-<j>.txt` for `i < folders`, `j < files`: a large tree in one go
   * (on the phone one shell loop, not a push per file). */
  tree: (sub: string, folders: number, files: number) => void
  /** Every file below `sub` (the whole folder for `''`), as paths relative to the folder. */
  files: (sub?: string) => string[]
  /** The files below `sub` (as `files` names them) that hold any of `texts`, searched where they
   * lie: on the phone one `grep`, not a pull per file. Pulling contents one after the other dropped
   * adb's connection to the emulator now and then, and the web view's DevTools forward with it. */
  containing: (sub: string, texts: string[]) => string[]
  remove: () => void
}

/**
 * The app's own folders in the shared storage on Android and its download folder there (Tauri's
 * download folder, `getExternalFilesDir(DIRECTORY_DOWNLOADS)`). Not the app's private storage: on
 * Android all of it is holzi's own data, which the file browser only reads (spec 044 FR-037), and
 * only `run-as` reaches it (a read-back through its `exec-out` once differed from the file in CI).
 * The app needs no permission for this folder, a chosen path may lie in it (contract
 * picked-file.md), adb reaches it directly, and `pm clear` before each scenario empties it.
 */
const ANDROID_FILES = `/storage/emulated/0/Android/data/${PACKAGE}/files`
const ANDROID_DOWNLOADS = `${ANDROID_FILES}/Download`

function androidAdb(env: NodeJS.ProcessEnv): Adb | undefined {
  const serial = env.ANDROID_SERIAL
  return env.E2E_PLATFORM === 'android' && serial !== undefined && serial !== ''
    ? createAdb(serial)
    : undefined
}

const quote = (text: string) => `'${text.replaceAll("'", "'\\''")}'`

export function deviceFiles(
  prefix: string,
  env: NodeJS.ProcessEnv = process.env,
): DeviceFiles {
  const adb = androidAdb(env)
  if (adb === undefined) {
    const folder = mkdtempSync(join(tmpdir(), prefix))
    const path = (name: string) => join(folder, name)
    return {
      folder,
      path,
      write: (name, content) => {
        mkdirSync(dirname(path(name)), { recursive: true })
        writeFileSync(path(name), content)
      },
      read: (name) => readFileSync(path(name), 'utf8'),
      readBytes: (name) => readFileSync(path(name)),
      tree: (sub, folders, files) => {
        for (let i = 0; i < folders; i++) {
          const dir = join(folder, sub, `d${i}`)
          mkdirSync(dir, { recursive: true })
          for (let j = 0; j < files; j++)
            writeFileSync(join(dir, `f${i}-${j}.txt`), '')
        }
      },
      files: (sub = '') =>
        readdirSync(join(folder, sub), { recursive: true, withFileTypes: true })
          .filter((entry) => entry.isFile())
          .map((entry) => relative(folder, join(entry.parentPath, entry.name)))
          .sort(),
      containing: (sub, texts) =>
        readdirSync(join(folder, sub), { recursive: true, withFileTypes: true })
          .filter((entry) => entry.isFile())
          .map((entry) => relative(folder, join(entry.parentPath, entry.name)))
          .filter((name) => {
            const body = readFileSync(path(name))
            return texts.some((text) => body.includes(text))
          })
          .sort(),
      remove: () => rmSync(folder, { recursive: true, force: true }),
    }
  }
  const folder = `${ANDROID_DOWNLOADS}/${prefix}${randomBytes(4).toString('hex')}`
  const path = (name: string) => `${folder}/${name}`
  /** Runs `use` with a fresh file of this machine and removes it afterwards. */
  const throughLocal = <T>(use: (local: string) => T): T => {
    const dir = mkdtempSync(join(tmpdir(), 'holzi-e2e-adb-'))
    try {
      return use(join(dir, 'content'))
    } finally {
      rmSync(dir, { recursive: true, force: true })
    }
  }
  const pull = (name: string) =>
    throughLocal((local) => {
      adb.pull(path(name), local)
      return readFileSync(local)
    })
  return {
    folder,
    path,
    write: (name, content) => {
      throughLocal((local) => {
        writeFileSync(local, content)
        adb.shell('mkdir', '-p', dirname(path(name)))
        adb.push(local, path(name))
      })
      // The shell user creates the file and its parent folders: open them up so the app may read,
      // change and remove them. The app's own entries are not the shell's to change.
      adb.shell(
        `chmod a+rwx ${quote(ANDROID_FILES)} ${quote(ANDROID_DOWNLOADS)} 2>/dev/null;`,
        `chmod -R a+rwX ${quote(folder)} 2>/dev/null; true`,
      )
    },
    read: (name) => pull(name).toString(),
    readBytes: pull,
    // One shell loop for the whole tree (a push per file would take minutes), opened up for the
    // app as `write` does.
    tree: (sub, folders, files) =>
      void adb.shell(
        'sh',
        '-c',
        quote(
          `mkdir -p ${quote(folder)} && cd ${quote(folder)} && i=0; while [ $i -lt ${folders} ]; do ` +
            `mkdir -p ${quote(sub)}/d$i && j=0; while [ $j -lt ${files} ]; do ` +
            `: > ${quote(sub)}/d$i/f$i-$j.txt; j=$((j+1)); done; i=$((i+1)); done; ` +
            `chmod a+rwx ${quote(ANDROID_FILES)} ${quote(ANDROID_DOWNLOADS)} 2>/dev/null; ` +
            `chmod -R a+rwX ${quote(folder)} 2>/dev/null; true`,
        ),
      ),
    files: (sub = '') =>
      adb
        .shell('find', quote(sub ? `${folder}/${sub}` : folder), '-type', 'f')
        .split('\n')
        .map((line) => line.trim())
        .filter((line) => line.startsWith(`${folder}/`))
        .map((line) => line.slice(folder.length + 1))
        .sort(),
    // grep exits with 1 when no file matches, with 2 when it cannot read one: only that fails.
    containing: (sub, texts) =>
      adb
        .shell(
          'grep',
          '-rlF',
          ...texts.flatMap((text) => ['-e', quote(text)]),
          '--',
          quote(sub ? `${folder}/${sub}` : folder),
          ';',
          '[ $? -le 1 ]',
        )
        .split('\n')
        .map((line) => line.trim())
        .filter((line) => line.startsWith(`${folder}/`))
        .map((line) => line.slice(folder.length + 1))
        .sort(),
    remove: () => void adb.shell('rm', '-rf', quote(folder)),
  }
}

/**
 * A file of this machine at a path the app on `page` can read: the same path on Linux, a copy in the
 * app's download folder on the phone (it stays until the app's data is cleared with the next
 * scenario). In an Android run the other devices of a group run on Linux (stage 2); only a page with
 * the phone's controls is on the phone.
 */
export function onDevice(
  hostPath: string,
  page: object,
  env: NodeJS.ProcessEnv = process.env,
): string {
  const onPhone = 'phone' in page && page.phone !== undefined
  if (!onPhone || androidAdb(env) === undefined) return hostPath
  const files = deviceFiles('e2e-staged-', env)
  files.write(basename(hostPath), readFileSync(hostPath))
  return files.path(basename(hostPath))
}
