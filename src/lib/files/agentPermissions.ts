// The file permissions of agents as the settings show them (spec 044 FR-031 to FR-031b, data-model.md
// `agent_file_permissions`). Pure, so `scripts/check-files-agent-permissions.ts` runs it without vue.
// No row is a decision too: the built-in agent reaches the device, nobody reaches a storage or an
// encrypted folder in it (spec 048 FR-030) before the user answered its question.

/** `agent_id` of the agent of the holzi chat (`files::permissions::BUILTIN_AGENT`). */
export const BUILTIN_AGENT = 'builtin'

/** One stored row (`AgentFilePermission` of the bindings, of which this needs only these fields). */
export type PermissionRow = {
  agentId: string
  kind: 'device' | 'storage' | 'encryptedFolder'
  /** Empty for the device, the storage id, or `<storage_id>/<folder_id>` for a folder. */
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
  status: 'read' | 'readWrite' | 'denied'
  reach: 'local' | 'cloud'
}

/** A storage without a row asks first. */
export type StorageChoice = 'ask' | 'read' | 'readWrite' | 'denied'

export type AgentFiles = {
  agentId: string
  device: boolean
  storages: { id: string; name: string; status: StorageChoice }[]
  /** By storage name, then folder name; folders of a storage that is gone are left out. */
  folders: FolderGrant[]
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
          status:
            status === 'read' || status === 'readWrite' || status === 'denied'
              ? status
              : 'ask',
        }
      }),
      folders: own
        .flatMap((row): FolderGrant[] => {
          const storage = storages.find((candidate) =>
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
