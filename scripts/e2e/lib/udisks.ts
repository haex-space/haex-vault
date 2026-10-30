// Loop images as drives for the drives scenario (spec 064, T034, research R12): a vfat image is made
// in a temporary folder and attached through udisks2 as the runner user, which is how a plugged-in
// stick looks to haex. The CI allows the runner user udisks2's actions with a polkit rule; without a
// working udisks2 the scenario is skipped. An encrypted stick (US3) needs `cryptsetup` and `mkfs`
// as root, through `sudo` without a password as on the CI runner.
import { execFileSync, spawnSync } from 'node:child_process'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

let available: boolean | undefined

/** Whether `udisksctl` reaches udisks2 here. */
export function udisksAvailable(): boolean {
  available ??=
    spawnSync('udisksctl', ['status'], { stdio: 'ignore', timeout: 20_000 })
      .status === 0 &&
    spawnSync('mkfs.vfat', ['--help'], { stdio: 'ignore', timeout: 5_000 })
      .status === 0 &&
    spawnSync('findmnt', ['--version'], { stdio: 'ignore', timeout: 5_000 })
      .status === 0 &&
    spawnSync('truncate', ['--version'], { stdio: 'ignore', timeout: 5_000 })
      .status === 0
  return available
}

let luks: boolean | undefined

/** Whether an encrypted stick can be made here: `cryptsetup` through `sudo` without a password. */
export function luksAvailable(): boolean {
  luks ??=
    udisksAvailable() &&
    spawnSync('sudo', ['-n', 'cryptsetup', '--version'], {
      stdio: 'ignore',
      timeout: 20_000,
    }).status === 0
  return luks
}

function run(command: string, args: string[]): string {
  return execFileSync(command, args, { encoding: 'utf8', timeout: 60_000 })
}

/** `Mapped file … as /dev/loop3.` → `/dev/loop3`. */
export function deviceOf(output: string): string {
  const device = /as (\/dev\/[a-z0-9-]+)\.?\s*$/.exec(output.trim())?.[1]
  if (!device) throw new Error(`no device in udisksctl output: ${output}`)
  return device
}

/** A 32 MB vfat image labelled `label` in `dir`. */
export function makeVfatImage(dir: string, label: string): string {
  const file = join(dir, `${label}.img`)
  run('truncate', ['-s', '32M', file])
  run('mkfs.vfat', ['-n', label, file])
  return file
}

/** A stick image in a temporary folder of its own; `remove` deletes both. */
export interface StickImage {
  file: string
  remove(): void
}

/** A vfat stick image labelled `label`. */
export function makeStick(label: string): StickImage {
  const dir = mkdtempSync(join(tmpdir(), 'haex-drives-'))
  return {
    file: makeVfatImage(dir, label),
    remove: () => rmSync(dir, { recursive: true, force: true }),
  }
}

/** Writes a file onto a mounted drive, as another program would. */
export function writeOnDrive(mountPoint: string, name: string, text: string) {
  writeFileSync(join(mountPoint, name), text)
}

/** Attaches `file` as a loop device, as if it were plugged in. */
export function attach(file: string): string {
  return deviceOf(
    run('udisksctl', ['loop-setup', '--no-user-interaction', '-f', file]),
  )
}

/** Where `device` is mounted now, or `null`. */
export function mountPointOf(device: string): string | null {
  const result = spawnSync('findmnt', ['-n', '-o', 'TARGET', device], {
    encoding: 'utf8',
    timeout: 20_000,
  })
  const target = result.stdout?.trim()
  return result.status === 0 && target ? target : null
}

/** Unmounts `device` if mounted and detaches it, as if it were pulled. Never throws. */
export function detach(device: string): void {
  spawnSync('udisksctl', ['unmount', '--no-user-interaction', '-b', device], {
    stdio: 'ignore',
    timeout: 60_000,
  })
  spawnSync(
    'udisksctl',
    ['loop-delete', '--no-user-interaction', '-b', device],
    {
      stdio: 'ignore',
      timeout: 60_000,
    },
  )
}

/** The object path of the unlocked content of the encrypted `device`, or `null` while it is locked. */
export function cleartextOf(device: string): string | null {
  const info = run('udisksctl', ['info', '-b', device])
  const path = /CleartextDevice:\s+'([^']*)'/.exec(info)?.[1]
  return path && path !== '/' ? path : null
}

/** Locks the encrypted `device` (unmounting its content first) and detaches it. Never throws. */
export function detachEncrypted(device: string): void {
  try {
    const cleartext = cleartextOf(device)
    if (cleartext) {
      const object = cleartext.replace('/org/freedesktop/UDisks2/', '')
      spawnSync(
        'udisksctl',
        ['unmount', '--no-user-interaction', '-p', object],
        { stdio: 'ignore', timeout: 60_000 },
      )
      spawnSync('udisksctl', ['lock', '--no-user-interaction', '-b', device], {
        stdio: 'ignore',
        timeout: 60_000,
      })
    }
  } catch {
    // Gone already.
  }
  detach(device)
}

/** An encrypted stick, attached and locked: the LUKS device and how to remove it. */
export interface LockedStick {
  device: string
  remove(): void
}

/**
 * A LUKS2 image labelled `label` whose content is a vfat file system of the same label holding
 * `files`, attached and locked again, as an encrypted stick looks when plugged in. The password goes
 * through a key file, never the command line.
 */
export function makeLockedStick(
  label: string,
  password: string,
  files: Record<string, string>,
): LockedStick {
  const dir = mkdtempSync(join(tmpdir(), 'haex-drives-'))
  const file = join(dir, `${label}.img`)
  const key = join(dir, 'key')
  let device: string | null = null
  const remove = () => {
    if (device) detachEncrypted(device)
    rmSync(dir, { recursive: true, force: true })
  }
  try {
    run('truncate', ['-s', '64M', file])
    writeFileSync(key, password)
    run('sudo', [
      '-n',
      'cryptsetup',
      'luksFormat',
      '-q',
      '--type',
      'luks2',
      '--pbkdf',
      'pbkdf2',
      '--label',
      label,
      '--key-file',
      key,
      file,
    ])
    device = attach(file)
    const clear = deviceOf(
      run('udisksctl', [
        'unlock',
        '--no-user-interaction',
        '-b',
        device,
        '--key-file',
        key,
      ]),
    )
    run('sudo', ['-n', 'mkfs.vfat', '-n', label, clear])
    spawnSync('udevadm', ['settle'], { stdio: 'ignore', timeout: 60_000 })
    run('udisksctl', ['mount', '--no-user-interaction', '-b', clear])
    const mountPoint = mountPointOf(clear)
    if (!mountPoint) throw new Error(`${clear} did not mount`)
    for (const [name, text] of Object.entries(files))
      writeOnDrive(mountPoint, name, text)
    run('udisksctl', ['unmount', '--no-user-interaction', '-b', clear])
    run('udisksctl', ['lock', '--no-user-interaction', '-b', device])
    return { device, remove }
  } catch (error) {
    remove()
    throw error
  }
}
