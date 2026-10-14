import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import { createAdb } from './adb.ts'
import type { AdbRunner } from './adb.ts'
import { contactControls, parseRows, quote } from './android-contacts.ts'

/** An adb that records every call and answers from `answers` (by the start of the joined call). */
function fakeAdb(answers: Record<string, string> = {}) {
  const calls: string[] = []
  const run: AdbRunner = (args) => {
    const line = args.join(' ')
    calls.push(line)
    const found = Object.entries(answers).find(([prefix]) =>
      line.startsWith(prefix),
    )
    return Buffer.from(found?.[1] ?? '')
  }
  return { calls, run }
}

describe('the phone address book of the scenarios', () => {
  it('quotes an argument for the device shell', () => {
    assert.equal(quote("data1:s:Anna O'Neil"), `'data1:s:Anna O'\\''Neil'`)
  })

  it('reads rows of content query output, values with commas included', () => {
    const rows = parseRows(
      'Row: 0 raw_contact_id=5, mimetype=vnd.android.cursor.item/name, data1=Beck, Anna\nRow: 1 raw_contact_id=6, mimetype=x, data1=NULL\n',
      ['raw_contact_id', 'mimetype', 'data1'],
    )
    assert.deepEqual(rows, [
      {
        raw_contact_id: '5',
        mimetype: 'vnd.android.cursor.item/name',
        data1: 'Beck, Anna',
      },
      { raw_contact_id: '6', mimetype: 'x', data1: 'NULL' },
    ])
  })

  it('writes a raw contact and its rows as the user does', () => {
    const fake = fakeAdb({
      '-s emu shell content query --uri content://com.android.contacts/raw_contacts --projection _id --sort':
        'Row: 0 _id=41\n',
    })
    const id = contactControls(createAdb('emu', fake.run)).insertContact({
      account: ['com.holzi.e2e', 'test'],
      given: 'Anna',
      family: 'Beck',
      phones: ['0170 1234567'],
    })
    assert.equal(id, 41)
    assert.ok(fake.calls[0]?.includes("'account_type:s:com.holzi.e2e'"))
    assert.ok(fake.calls.some((call) => call.includes("'data1:s:Anna Beck'")))
    assert.ok(
      fake.calls.some(
        (call) =>
          call.includes('mimetype:s:vnd.android.cursor.item/phone_v2') &&
          call.includes("'data1:s:0170 1234567'"),
      ),
    )
  })

  it('reads the contacts that are not deleted', () => {
    const fake = fakeAdb({
      '-s emu shell content query --uri content://com.android.contacts/raw_contacts':
        'Row: 0 _id=5\n',
      '-s emu shell content query --uri content://com.android.contacts/data':
        'Row: 0 raw_contact_id=5, mimetype=vnd.android.cursor.item/name, data1=Anna Beck\n' +
        'Row: 1 raw_contact_id=5, mimetype=vnd.android.cursor.item/phone_v2, data1=0170 1\n' +
        'Row: 2 raw_contact_id=7, mimetype=vnd.android.cursor.item/name, data1=Gone\n',
    })
    assert.deepEqual(
      contactControls(createAdb('emu', fake.run)).queryContacts(),
      [{ id: 5, name: 'Anna Beck', phones: ['0170 1'], emails: [] }],
    )
  })

  it('grants without ending the app and refuses for good', () => {
    const fake = fakeAdb()
    const controls = contactControls(createAdb('emu', fake.run))
    controls.allowContacts()
    controls.refuseContacts()
    assert.deepEqual(
      fake.calls.filter((call) => call.includes(' pm grant')),
      [
        '-s emu shell pm grant com.haex.holzi android.permission.READ_CONTACTS',
        '-s emu shell pm grant com.haex.holzi android.permission.WRITE_CONTACTS',
      ],
    )
    assert.ok(
      fake.calls.some((call) =>
        call.endsWith('android.permission.READ_CONTACTS user-set user-fixed'),
      ),
    )
    assert.ok(!fake.calls.some((call) => call.includes(' pm revoke')))
  })

  it('removes every raw contact for good', () => {
    const fake = fakeAdb()
    contactControls(createAdb('emu', fake.run)).removeTestContacts()
    assert.deepEqual(fake.calls, [
      "-s emu shell content delete --uri 'content://com.android.contacts/raw_contacts?caller_is_syncadapter=true'",
    ])
  })
})
