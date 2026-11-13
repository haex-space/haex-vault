// A pass file for the import scenarios (spec 092, US6): a ZIP of stored (not compressed) entries
// built in code, so no archive library is needed and no real pass is in the repository. The data is
// made up; the signature and manifest are not needed by the reader.
import { crc32 } from 'node:zlib'

type Entries = Record<string, string | Uint8Array>

/** The bytes of a ZIP with the entries stored as they are. */
export function buildZip(entries: Entries): Buffer {
  const locals: Buffer[] = []
  const centrals: Buffer[] = []
  let offset = 0
  for (const [name, content] of Object.entries(entries)) {
    const data = Buffer.from(content)
    const nameBytes = Buffer.from(name)
    const checksum = crc32(data)
    const local = Buffer.alloc(30)
    local.writeUInt32LE(0x04034b50, 0)
    local.writeUInt16LE(20, 4)
    local.writeUInt32LE(checksum, 14)
    local.writeUInt32LE(data.length, 18)
    local.writeUInt32LE(data.length, 22)
    local.writeUInt16LE(nameBytes.length, 26)
    locals.push(local, nameBytes, data)
    const central = Buffer.alloc(46)
    central.writeUInt32LE(0x02014b50, 0)
    central.writeUInt16LE(20, 4)
    central.writeUInt16LE(20, 6)
    central.writeUInt32LE(checksum, 16)
    central.writeUInt32LE(data.length, 20)
    central.writeUInt32LE(data.length, 24)
    central.writeUInt16LE(nameBytes.length, 28)
    central.writeUInt32LE(offset, 42)
    centrals.push(central, nameBytes)
    offset += local.length + nameBytes.length + data.length
  }
  const centralBytes = Buffer.concat(centrals)
  const end = Buffer.alloc(22)
  end.writeUInt32LE(0x06054b50, 0)
  end.writeUInt16LE(Object.keys(entries).length, 8)
  end.writeUInt16LE(Object.keys(entries).length, 10)
  end.writeUInt32LE(centralBytes.length, 12)
  end.writeUInt32LE(offset, 16)
  return Buffer.concat([...locals, centralBytes, end])
}

/** A boarding pass of a made-up railway: a dated ticket, so the proposal gives it the tag. */
export function trainPass(serial: string, secret: string): Buffer {
  return buildZip({
    'pass.json': JSON.stringify({
      formatVersion: 1,
      passTypeIdentifier: 'pass.example.rail',
      serialNumber: serial,
      organizationName: 'Example Rail',
      description: 'Train ticket',
      logoText: 'ICE',
      relevantDate: '2030-11-14T08:30:00+01:00',
      expirationDate: '2030-11-14T23:59:00+01:00',
      webServiceURL: 'https://updates.example.invalid/passes',
      authenticationToken: 'e2e-token-token-token',
      barcodes: [{ format: 'PKBarcodeFormatQR', message: secret }],
      boardingPass: {
        transitType: 'PKTransitTypeTrain',
        primaryFields: [
          { key: 'from', label: 'From', value: 'Berlin' },
          { key: 'to', label: 'To', value: 'München' },
        ],
        auxiliaryFields: [{ key: 'seat', label: 'Seat', value: '42' }],
      },
    }),
  })
}

/** A store card without a date: the proposal gives it neither folder nor tag. */
export function storeCard(serial: string, secret: string): Buffer {
  return buildZip({
    'pass.json': JSON.stringify({
      formatVersion: 1,
      passTypeIdentifier: 'pass.example.shop',
      serialNumber: serial,
      organizationName: 'Corner Shop',
      description: 'Customer card',
      barcodes: [{ format: 'PKBarcodeFormatCode128', message: secret }],
      storeCard: { primaryFields: [] },
    }),
  })
}
