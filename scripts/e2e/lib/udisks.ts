// Loop images as drives for the drives scenario (spec 064, T034, research R12): a vfat image is made
// in a temporary folder and attached through udisks2 as the runner user, which is how a plugged-in
// stick looks to haex. The CI allows the runner user udisks2's actions with a polkit rule; without a
// working udisks2 the scenario is skipped.
import { execFileSync, spawnSync } from 'node:child_process'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

let available: boolean | undefined

/** Whether `udisksctl` reaches udisks2 here. */
export function udisksAvailable(): boolean {
  available ??=
    spawnSync('udisksctl', ['status'], { stdio: 'ignore', timeout: 20_000 })
      .status === 0
  return available
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
