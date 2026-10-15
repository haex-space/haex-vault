// The file permissions of agents as the settings show them (spec 044 FR-031 to FR-031b, data-model.md
// `agent_file_permissions`). Pure, so `scripts/check-files-agent-permissions.ts` runs it without vue.
// No row is a decision too: the built-in agent reaches the device, nobody reaches a storage or an
// encrypted folder in it (spec 048 FR-030) or the contacts (spec 051 FR-030) before the user
// answered its question.

/** `agent_id` of the agent of the holzi chat (`files::permissions::BUILTIN_AGENT`). */
export const BUILTIN_AGENT = 'builtin'

/** One stored row (`AgentFilePermission` of the bindings, of which this needs only these fields). */
export type PermissionRow = {
  agentId: string
  kind: 'device' | 'storage' | 'encryptedFolder' | 'contacts'
  /** Empty for the device, the storage id, `<storage_id>/<folder_id>` for a folder, `*` or
   * `group:<id>` for the contacts. */
  target: string
  status: 'granted' | 'read' | 'readWrite' | 'denied'
  /** Only for a folder: which models it covers. */
  reach?: 'local' | 'cloud'
  /** A folder's decrypted name, when this device has read it. */
  folderName?: string
}

/** A remembered answer for an encrypted folder. */
export type FolderGrant = {
  target: string
  name: string | null
  storageName: string
  /** The folder lies on this device (`device:<root>`, spec 048 US7), not in a storage. */
  onDevice: boolean
  status: 'read' | 'readWrite' | 'denied'
  reach: 'local' | 'cloud'
}

/** A storage without a row asks first. */
export type StorageChoice = 'ask' | 'read' | 'readWrite' | 'denied'

/** A remembered answer for the contacts of one group. */
export type ContactsGroupGrant = {
  target: string
  groupId: string
  status: 'read' | 'readWrite' | 'denied'
}

export type AgentFiles = {
  agentId: string
  device: boolean
  storages: { id: string; name: string; status: StorageChoice }[]
  /** All contacts (`*`); `ask` without a row. */
  contacts: StorageChoice
  contactGroups: ContactsGroupGrant[]
  /** By storage name (the device's first), then folder name; folders of a storage that is gone are
   * left out. */
  folders: FolderGrant[]
}

/** A stored status as a choice; no row (or another kind's status) asks. */
function choiceOf(status: PermissionRow['status'] | undefined): StorageChoice {
  return status === 'read' || status === 'readWrite' || status === 'denied'
    ? status
    : 'ask'
}

/**
 * One entry per agent: the built-in one first, then every other agent with a row, by id. Each lists
 * every storage in `storages`; rows of storages that are gone are left out.
 */
export function agentFiles(
  rows: readonly PermissionRow[],
  storages: readonly { id: string; name: string }[],
): AgentFiles[] {
  const others = [...new Set(rows.map((row) => row.agentId))]
    .filter((id) => id !== BUILTIN_AGENT)
    .sort()
  return [BUILTIN_AGENT, ...others].map((agentId) => {
    const own = rows.filter((row) => row.agentId === agentId)
    const device = own.find((row) => row.kind === 'device')?.status
    return {
      agentId,
      device:
        device === undefined ? agentId === BUILTIN_AGENT : device === 'granted',
      storages: storages.map((storage) => {
        const status = own.find(
          (row) => row.kind === 'storage' && row.target === storage.id,
        )?.status
        return {
          id: storage.id,
          name: storage.name,
          status: choiceOf(status),
        }
      }),
      contacts: choiceOf(
        own.find((row) => row.kind === 'contacts' && row.target === '*')
          ?.status,
      ),
      contactGroups: own.flatMap((row): ContactsGroupGrant[] => {
        const groupId = row.target.startsWith('group:')
          ? row.target.slice('group:'.length)
          : ''
        const status = choiceOf(row.status)
        if (row.kind !== 'contacts' || !groupId || status === 'ask') return []
        return [{ target: row.target, groupId, status }]
      }),
      folders: own
        .flatMap((row): FolderGrant[] => {
          const onDevice = row.target.startsWith('device:')
          const storage = onDevice
            ? { name: '' }
            : storages.find((candidate) =>
                row.target.startsWith(`${candidate.id}/`),
              )
          if (
            row.kind !== 'encryptedFolder' ||
            !storage ||
            row.status === 'granted'
          )
            return []
          return [
            {
              target: row.target,
              name: row.folderName ?? null,
              storageName: storage.name,
              onDevice,
              status: row.status,
              reach: row.reach ?? 'local',
            },
          ]
        })
        .sort(
          (a, b) =>
            a.storageName.localeCompare(b.storageName) ||
            (a.name ?? '').localeCompare(b.name ?? ''),
        ),
    }
  })
}
