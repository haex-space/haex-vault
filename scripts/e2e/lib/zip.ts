// A ZIP archive of stored (uncompressed) entries, built in memory, for scenarios that hand the app
// an archive through a file the dialog would have chosen (spec 053: an icon set).
import { crc32 } from 'node:zlib'

export type ZipEntry = { name: string; data: Uint8Array | string }

function u16(value: number): Buffer {
  const out = Buffer.alloc(2)
  out.writeUInt16LE(value)
  return out
}

function u32(value: number): Buffer {
  const out = Buffer.alloc(4)
  out.writeUInt32LE(value >>> 0)
  return out
}

/** The bytes of a ZIP with these entries, in this order. */
export function zipOf(entries: readonly ZipEntry[]): Buffer {
  const locals: Buffer[] = []
  const centrals: Buffer[] = []
  let offset = 0
  for (const entry of entries) {
    const name = Buffer.from(entry.name, 'utf8')
    const data = Buffer.from(entry.data)
    const crc = crc32(data)
    const shared = [
      u16(20), // version needed
      u16(0x0800), // flags: the name is UTF-8
      u16(0), // stored
      u16(0), // time
      u16(0x21), // date: 1980-01-01
      u32(crc),
      u32(data.length),
      u32(data.length),
      u16(name.length),
    ]
    const local = Buffer.concat([
      u32(0x04034b50),
      ...shared,
      u16(0),
      name,
      data,
    ])
    centrals.push(
      Buffer.concat([
        u32(0x02014b50),
        u16(20),
        ...shared,
        u16(0), // extra length
        u16(0), // comment length
        u16(0), // disk
        u16(0), // internal attributes
        u32(0), // external attributes
        u32(offset),
        name,
      ]),
    )
    locals.push(local)
    offset += local.length
  }
  const central = Buffer.concat(centrals)
  return Buffer.concat([
    ...locals,
    central,
    u32(0x06054b50),
    u16(0),
    u16(0),
    u16(entries.length),
    u16(entries.length),
    u32(central.length),
    u32(offset),
    u16(0),
  ])
}
