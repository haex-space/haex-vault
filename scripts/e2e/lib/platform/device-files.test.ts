import assert from 'node:assert/strict'
import { describe, it } from 'node:test'
import { deviceFiles } from './device-files.ts'

describe('deviceFiles on Linux', () => {
  it('names the files below a folder that hold any of the texts', () => {
    const files = deviceFiles('holzi-device-files-test-', {})
    try {
      files.write('a.txt', 'nichts')
      files.tree('sub', 1, 1)
      files.write('sub/d0/f0-0.txt', 'Hallo aus dem verschlüsselten Ordner')
      files.write(
        'sub/d0/b.bin',
        Buffer.from([0, 1, 2, 0x66, 0x69, 0x6c, 0x6d, 3]),
      )
      assert.deepEqual(files.containing('sub', ['verschlüsselten', 'film']), [
        'sub/d0/b.bin',
        'sub/d0/f0-0.txt',
      ])
      assert.deepEqual(files.containing('', ['Tresor']), [])
    } finally {
      files.remove()
    }
  })
})
