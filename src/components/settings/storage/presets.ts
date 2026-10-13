/**
 * The providers the connection form offers per protocol, with what they prefill (spec 038 US1,
 * spec 050 research R11). Parts in `<…>` are for the user to fill in; the form marks them.
 */
import type { Addressing } from '@bindings/Addressing'
import type { Protocol } from '@bindings/Protocol'
import type { ProviderKind } from '@bindings/ProviderKind'

/** The protocols this form offers (spec 050). */
export type FormProtocol = Extract<Protocol, 's3' | 'webdav' | 'sftp'>

export type Preset = {
  endpoint: string
  addressing: Addressing
  region: string
}

/** The providers of each protocol, in the order of the choice, with what they prefill. */
export const PRESETS: Record<
  FormProtocol,
  Partial<Record<ProviderKind, Preset>>
> = {
  s3: {
    rustfs: {
      endpoint: 'http://127.0.0.1:9000',
      addressing: 'path',
      region: 'us-east-1',
    },
    aws: { endpoint: '', addressing: 'virtual', region: 'eu-central-1' },
    other: { endpoint: 'https://', addressing: 'path', region: 'us-east-1' },
  },
  webdav: {
    hetznerStorageBox: {
      endpoint: 'https://<benutzer>.your-storagebox.de/',
      addressing: 'path',
      region: '',
    },
    nextcloud: {
      endpoint: 'https://<server>/remote.php/dav/files/<benutzer>/',
      addressing: 'path',
      region: '',
    },
    koofr: {
      endpoint: 'https://app.koofr.net/dav/Koofr/',
      addressing: 'path',
      region: '',
    },
    kdrive: {
      endpoint: 'https://<kdrive-id>.connect.kdrive.infomaniak.com/',
      addressing: 'path',
      region: '',
    },
    other: { endpoint: 'https://', addressing: 'path', region: '' },
  },
  sftp: {
    // Port 23 takes keys as OpenSSH lines; port 22 wants RFC 4716 (research R11).
    hetznerStorageBox: {
      endpoint: 'sftp://<benutzer>.your-storagebox.de:23',
      addressing: 'path',
      region: '',
    },
    other: { endpoint: 'sftp://', addressing: 'path', region: '' },
  },
}
