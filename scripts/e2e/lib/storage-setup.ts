// A storage of spec 038 on the rig's RustFS, connected as the user would in the settings (spec 044
// files-storage scenarios).
import { unwrap, type FlowInstance } from './flows.ts'
import type { Rustfs } from './rustfs.ts'
import type { Webdav } from './webdav.ts'
import { runAction, wmSnapshot } from './settings.ts'

/** Saves a connection to `rustfs` and a storage `name` on `bucket`; returns the storage's id. */
export async function connectStorage(
  instance: Pick<FlowInstance, 'invoke'>,
  rustfs: Rustfs,
  bucket: string,
  name: string,
): Promise<string> {
  const connection = unwrap<{ id: string }>(
    'storage_connection_save',
    await instance.invoke('storage_connection_save', {
      input: {
        protocol: 's3',
        providerName: 'RustFS',
        providerKind: 'rustfs',
        endpoint: rustfs.endpoint,
        region: rustfs.region,
        addressing: 'path',
        credentials: {
          kind: 's3',
          accessKeyId: rustfs.accessKeyId,
          secretAccessKey: rustfs.secretAccessKey,
        },
        bucketForTest: bucket,
      },
    }),
  )
  const storage = unwrap<{ id: string }>(
    'storage_save',
    await instance.invoke('storage_save', {
      input: { connectionId: connection.id, name, bucket },
    }),
  )
  return storage.id
}

/** Saves a connection to the WebDAV server `webdav` and a storage `name` on its folder `folder`,
 * made when missing (spec 050); returns the storage's id. */
export async function connectWebdav(
  instance: Pick<FlowInstance, 'invoke'>,
  webdav: Webdav,
  folder: string,
  name: string,
): Promise<string> {
  const connection = unwrap<{ id: string }>(
    'storage_connection_save',
    await instance.invoke('storage_connection_save', {
      input: {
        protocol: 'webdav',
        providerName: 'rclone',
        providerKind: 'other',
        endpoint: webdav.endpoint,
        region: '',
        addressing: 'path',
        credentials: {
          kind: 'password',
          username: webdav.username,
          password: webdav.password,
        },
        bucketForTest: folder,
        createFolder: true,
      },
    }),
  )
  const storage = unwrap<{ id: string }>(
    'storage_save',
    await instance.invoke('storage_save', {
      input: { connectionId: connection.id, name, bucket: folder },
    }),
  )
  return storage.id
}

/** Moves the file browser's tab to `at` (`/device?p=…`, `/storage/<id>?p=…`) in place: a second
 * window would lie over the first one while it opens and catch the next click. */
export async function filesGo(
  instance: FlowInstance,
  at: string,
): Promise<void> {
  const snapshot = await wmSnapshot(instance)
  const tab = snapshot.windows
    .flatMap((window) => window.tabs)
    .find((candidate) => candidate.appId === 'system.files')
  if (!tab) throw new Error('no file browser tab')
  const moved = await runAction(instance, 'wm.tab.navigate', {
    tabId: tab.id,
    to: at,
  })
  if (!moved.ok) throw new Error(`wm.tab.navigate: ${JSON.stringify(moved)}`)
}
