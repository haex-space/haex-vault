// Part of `pnpm check:files` (spec 064, T016, T031): which drive a path lies on, moving a path to a
// new mount point, and the nearest folder that still exists.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  type DriveLike,
  driveLabel,
  driveOf,
  isCurrentDrive,
  isUnder,
  nearestExisting,
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
