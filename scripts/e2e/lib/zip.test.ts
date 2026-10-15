import assert from 'node:assert/strict'
import { crc32 } from 'node:zlib'
import { describe, it } from 'node:test'
import { zipOf } from './zip.ts'

describe('zipOf', () => {
  it('writes the entries of a ZIP that a reader finds again', () => {
    const bytes = zipOf([
      { name: 'a.txt', data: 'hello' },
      { name: 'dir/b.bin', data: new Uint8Array([1, 2, 3]) },
    ])
    const end = bytes.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]))
    assert.equal(bytes.readUInt16LE(end + 10), 2, 'two entries')
    const centralOffset = bytes.readUInt32LE(end + 16)
    assert.equal(bytes.readUInt32LE(centralOffset), 0x02014b50)
    // The first entry: its CRC, size and name where the central directory says.
    assert.equal(bytes.readUInt32LE(centralOffset + 16), crc32('hello'))
    assert.equal(bytes.readUInt32LE(centralOffset + 24), 5)
    const nameLength = bytes.readUInt16LE(centralOffset + 28)
    assert.equal(
      bytes
        .subarray(centralOffset + 46, centralOffset + 46 + nameLength)
        .toString(),
      'a.txt',
    )
    const local = bytes.readUInt32LE(centralOffset + 42)
    assert.equal(bytes.readUInt32LE(local), 0x04034b50)
    assert.equal(
      bytes.subarray(local + 30 + 5, local + 30 + 5 + 5).toString(),
      'hello',
    )
  })
})
