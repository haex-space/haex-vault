// The phone's address book in the Android scenarios (spec 051 T048): the permission as the person
// gives or refuses it, and raw contacts written and read "as the user" through `adb shell content`.
// `pm clear` does not touch the Contacts Provider, so every scenario removes its raw contacts.
import type { Adb } from './adb.ts'

const PACKAGE = 'com.haex.holzi'
const READ = 'android.permission.READ_CONTACTS'
const WRITE = 'android.permission.WRITE_CONTACTS'
const RAW_CONTACTS = 'content://com.android.contacts/raw_contacts'
const DATA = 'content://com.android.contacts/data'
const MIME = {
  name: 'vnd.android.cursor.item/name',
  phone: 'vnd.android.cursor.item/phone_v2',
  email: 'vnd.android.cursor.item/email_v2',
} as const

/** A contact as a scenario writes it. */
export interface PhoneContactInput {
  /** `[type, name]`; without it the contact is in the phone's local account. */
  account?: [string, string]
  given: string
  family: string
  phones?: string[]
  emails?: string[]
}

/** A raw contact as a scenario reads it back. */
export interface PhoneContact {
  id: number
  name: string
  phones: string[]
  emails: string[]
}

export interface ContactControls {
  /** Grants reading and writing the address book, as the person's "Allow" does. */
  allowContacts(): void
  /** Refuses it for good, as the person's second "Don't allow" does (FR-017). */
  refuseContacts(): void
  /** Takes a granted permission back, as the system's settings do; the app's process ends. */
  revokeContacts(): void
  /** Writes a raw contact "as the user" and answers its id. */
  insertContact(contact: PhoneContactInput): number
  /** The raw contacts that are not deleted, with name, numbers and addresses. */
  queryContacts(): PhoneContact[]
  /** Replaces the first number of a raw contact "as the user". */
  updatePhone(id: number, number: string): void
  /** Deletes a raw contact "as the user". */
  deleteContact(id: number): void
  /** Removes every raw contact for good (a scenario's clean-up). */
  removeTestContacts(): void
}

/** One argument of the device's shell command line, quoted: adb joins the arguments into one. */
export function quote(text: string): string {
  return `'${text.replaceAll("'", "'\\''")}'`
}

/** The rows of `content query` output, with the columns of `columns` in that order. */
export function parseRows(
  output: string,
  columns: readonly string[],
): Record<string, string>[] {
  return output
    .split('\n')
    .filter((line) => line.startsWith('Row: '))
    .map((line) => {
      const row: Record<string, string> = {}
      let rest = line.replace(/^Row: \d+ /, '')
      for (const [index, column] of columns.entries()) {
        const next = columns[index + 1]
        const start = `${column}=`
        if (!rest.startsWith(start)) break
        const end = next === undefined ? -1 : rest.indexOf(`, ${next}=`)
        row[column] =
          end < 0 ? rest.slice(start.length) : rest.slice(start.length, end)
        rest = end < 0 ? '' : rest.slice(end + 2)
      }
      return row
    })
}

export function contactControls(adb: Adb): ContactControls {
  const content = (...args: string[]) => adb.shell('content', ...args)
  const insertData = (
    raw: number,
    mimetype: string,
    binds: Record<string, string>,
  ) =>
    content(
      'insert',
      '--uri',
      DATA,
      '--bind',
      `raw_contact_id:i:${raw}`,
      '--bind',
      `mimetype:s:${mimetype}`,
      ...Object.entries(binds).flatMap(([key, value]) => [
        '--bind',
        quote(`${key}:s:${value}`),
      ]),
    )
  const newestRaw = () => {
    const rows = parseRows(
      content(
        'query',
        '--uri',
        RAW_CONTACTS,
        '--projection',
        '_id',
        '--sort',
        quote('_id DESC'),
      ),
      ['_id'],
    )
    const id = Number(rows[0]?._id)
    if (!Number.isInteger(id)) throw new Error('the new raw contact has no id')
    return id
  }
  return {
    allowContacts: () => {
      for (const permission of [READ, WRITE]) {
        adb.shell(
          'pm',
          'clear-permission-flags',
          PACKAGE,
          permission,
          'user-fixed',
        )
        adb.shell('pm', 'grant', PACKAGE, permission)
      }
    },
    // Not granted until now, so marking it decided does not end the app.
    refuseContacts: () => {
      for (const permission of [READ, WRITE]) {
        adb.shell(
          'pm',
          'set-permission-flags',
          PACKAGE,
          permission,
          'user-set',
          'user-fixed',
        )
      }
    },
    revokeContacts: () => {
      for (const permission of [READ, WRITE]) {
        adb.shell('pm', 'revoke', PACKAGE, permission)
      }
    },
    insertContact: (contact) => {
      const account = contact.account
        ? [
            '--bind',
            quote(`account_type:s:${contact.account[0]}`),
            '--bind',
            quote(`account_name:s:${contact.account[1]}`),
          ]
        : // An insert needs a binding; the local account has none.
          ['--bind', 'account_type:n:', '--bind', 'account_name:n:']
      content('insert', '--uri', RAW_CONTACTS, ...account)
      const raw = newestRaw()
      insertData(raw, MIME.name, {
        data1: `${contact.given} ${contact.family}`,
        data2: contact.given,
        data3: contact.family,
      })
      for (const number of contact.phones ?? []) {
        insertData(raw, MIME.phone, { data1: number, data2: '2' })
      }
      for (const address of contact.emails ?? []) {
        insertData(raw, MIME.email, { data1: address, data2: '1' })
      }
      return raw
    },
    queryContacts: () => {
      const live = parseRows(
        content(
          'query',
          '--uri',
          RAW_CONTACTS,
          '--projection',
          '_id',
          '--where',
          quote('deleted=0'),
        ),
        ['_id'],
      ).map((row) => Number(row._id))
      const columns = ['raw_contact_id', 'mimetype', 'data1'] as const
      const rows = parseRows(
        content('query', '--uri', DATA, '--projection', columns.join(':')),
        columns,
      )
      return live.map((id) => {
        const of = (mimetype: string) =>
          rows
            .filter(
              (row) =>
                Number(row.raw_contact_id) === id && row.mimetype === mimetype,
            )
            .map((row) => row.data1 ?? '')
        return {
          id,
          name: of(MIME.name)[0] ?? '',
          phones: of(MIME.phone),
          emails: of(MIME.email),
        }
      })
    },
    updatePhone: (id, number) => {
      content(
        'update',
        '--uri',
        DATA,
        '--bind',
        quote(`data1:s:${number}`),
        '--where',
        quote(`raw_contact_id=${id} AND mimetype='${MIME.phone}'`),
      )
    },
    deleteContact: (id) => {
      content('delete', '--uri', RAW_CONTACTS, '--where', quote(`_id=${id}`))
    },
    // As the sync adapter: removed for good, not only marked deleted.
    removeTestContacts: () => {
      content(
        'delete',
        '--uri',
        quote(`${RAW_CONTACTS}?caller_is_syncadapter=true`),
      )
    },
  }
}
