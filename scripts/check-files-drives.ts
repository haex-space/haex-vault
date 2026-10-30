// Part of `pnpm check:files` (spec 064, T016, T031, T060): which drive a path lies on, moving a path
// to a new mount point, the drive a place remembers, and the nearest folder that still exists.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  type DriveLike,
  driveLabel,
  driveOf,
  isCurrentDrive,
  isUnder,
  nearestExisting,
  placeOnDrive,
  remap,
} from '../src/lib/files/drives.ts'

function drive(
  id: string,
  path: string | null,
  extra: Partial<DriveLike> = {},
): DriveLike {
  return {
    id,
    label: null,
    path,
    state: path === null ? 'unmounted' : 'mounted',
    size: null,
    ...extra,
  }
}

test('a path is under a root only on a separator boundary', () => {
  assert.equal(isUnder('/mnt/a/x', '/mnt/a'), true)
  assert.equal(isUnder('/mnt/a', '/mnt/a'), true)
  assert.equal(isUnder('/mnt/ab', '/mnt/a'), false)
  assert.equal(isUnder('/anything', '/'), true)
  assert.equal(isUnder('e:\\Fotos', 'E:\\'), true)
})

test('the drive of a path is the deepest mounted one above it', () => {
  const drives = [
    drive('root', '/'),
    drive('mnt', '/mnt/a'),
    drive('ab', '/mnt/ab'),
    drive('off', null),
  ]
  assert.equal(driveOf('/mnt/a/x/y', drives)?.id, 'mnt')
  assert.equal(driveOf('/mnt/ab', drives)?.id, 'ab')
  assert.equal(driveOf('/home/anna', drives)?.id, 'root')
  assert.equal(driveOf('/home', [drive('mnt', '/mnt/a')]), null)
})

test('windows drive letters match without case', () => {
  const drives = [drive('c', 'C:\\'), drive('e', 'E:\\')]
  assert.equal(driveOf('e:\\Fotos\\2026', drives)?.id, 'e')
})

test('a path moves to the new mount point of its drive', () => {
  assert.equal(
    remap(
      '/run/media/anna/BACKUP/ordner/x',
      '/run/media/anna/BACKUP',
      '/run/media/anna/BACKUP1',
    ),
    '/run/media/anna/BACKUP1/ordner/x',
  )
  assert.equal(remap('/media/a', '/media/a', '/media/b'), '/media/b')
  assert.equal(remap('/media/a/x', '/media/a', '/media/b/'), '/media/b/x')
  assert.equal(remap('/media/ab/x', '/media/a', '/media/b'), null)
  assert.equal(remap('E:\\Fotos', 'E:\\', 'F:\\'), 'F:\\Fotos')
})

test('a drive without a label is named by its size', () => {
  const unnamed = (size: number | null) => `Datenträger (${size ?? '?'})`
  assert.equal(
    driveLabel(drive('a', null, { label: 'FOTOS' }), unnamed),
    'FOTOS',
  )
  assert.equal(
    driveLabel(drive('a', null, { label: '', size: 32 }), unnamed),
    'Datenträger (32)',
  )
  assert.equal(
    driveLabel(drive('a', null, { size: null }), unnamed),
    'Datenträger (?)',
  )
})

test('the current drive is the one holding the open folder', () => {
  const drives = [drive('root', '/'), drive('stick', '/media/s')]
  assert.equal(isCurrentDrive(drives[1]!, '/media/s/x', drives), true)
  assert.equal(isCurrentDrive(drives[0]!, '/media/s/x', drives), false)
  assert.equal(isCurrentDrive(drives[0]!, null, drives), false)
})

test('the nearest existing folder is found above a vanished one', async () => {
  const there = new Set(['/', '/home', '/home/anna'])
  const asked: string[] = []
  const exists = async (path: string) => {
    asked.push(path)
    return there.has(path)
  }
  assert.equal(
    await nearestExisting('/home/anna/stick/a/b', exists),
    '/home/anna',
  )
  assert.deepEqual(asked, [
    '/home/anna/stick/a',
    '/home/anna/stick',
    '/home/anna',
  ])
  assert.equal(await nearestExisting('/x/y', async () => false), null)
})

test('a place on a drive with a key remembers that drive and its mount path', () => {
  const drives = [
    { ...drive('a', '/run/media/anna/BACKUP'), key: 'uuid-a' },
    { ...drive('b', '/run/media/anna/NOKEY'), key: null },
  ]
  const device = { kind: 'device' } as const
  assert.deepEqual(
    placeOnDrive({ source: device, path: '/run/media/anna/BACKUP/x' }, drives),
    {
      source: device,
      path: '/run/media/anna/BACKUP/x',
      drive: 'uuid-a',
      droot: '/run/media/anna/BACKUP',
    },
  )
  assert.deepEqual(
    placeOnDrive({ source: device, path: '/run/media/anna/NOKEY/x' }, drives),
    { source: device, path: '/run/media/anna/NOKEY/x' },
    'no key, no drive',
  )
  assert.deepEqual(
    placeOnDrive(
      {
        source: device,
        path: '/home/anna',
        drive: 'uuid-a',
        droot: '/run/media/anna/BACKUP',
      },
      drives,
    ),
    { source: device, path: '/home/anna' },
    'a place that left the drive forgets it',
  )
  const storage = { kind: 'storage', storageId: 'abc' } as const
  assert.deepEqual(
    placeOnDrive({ source: storage, path: '/run/media/anna/BACKUP' }, drives),
    { source: storage, path: '/run/media/anna/BACKUP' },
  )
})

test('a place that names a drive above its path keeps it, listed or not', () => {
  const place = {
    source: { kind: 'device' } as const,
    path: '/run/media/anna/BACKUP1/x',
    drive: 'uuid-a',
    droot: '/run/media/anna/BACKUP1',
  }
  assert.deepEqual(placeOnDrive(place, []), place)
})

test('the system drive never tags a place, and a deeper drive replaces the one it named', () => {
  const device = { kind: 'device' } as const
  const root = { ...drive('root', '/'), key: 'root-uuid', kind: 'system' }
  const stick = { ...drive('s', '/media/anna/STICK'), key: 'stick-uuid' }
  assert.deepEqual(
    placeOnDrive({ source: device, path: '/home/anna' }, [root]),
    { source: device, path: '/home/anna' },
    'the system drive is always there at a restore',
  )
  // A place that named a shallower drive (here as an old session left it) moves to the stick.
  assert.deepEqual(
    placeOnDrive(
      {
        source: device,
        path: '/media/anna/STICK/ordner',
        drive: 'root-uuid',
        droot: '/',
      },
      [root, stick],
    ),
    {
      source: device,
      path: '/media/anna/STICK/ordner',
      drive: 'stick-uuid',
      droot: '/media/anna/STICK',
    },
  )
  // While the list lags behind a mount, the deeper drive the place names stays.
  const named = {
    source: device,
    path: '/media/anna/STICK/ordner',
    drive: 'stick-uuid',
    droot: '/media/anna/STICK',
  }
  assert.deepEqual(placeOnDrive(named, [root]), named)
})
