// The file permissions of agents as the settings show them (spec 044 FR-031 to FR-031b, data-model.md
// `agent_file_permissions`). Pure, so `scripts/check-files-agent-permissions.ts` runs it without vue.
// No row is a decision too: the built-in agent reaches the device, nobody reaches a storage before
// the user answered its question.

/** `agent_id` of the agent of the holzi chat (`files::permissions::BUILTIN_AGENT`). */
export const BUILTIN_AGENT = 'builtin'

/** One stored row (`AgentFilePermission` of the bindings, of which this needs only these fields). */
export type PermissionRow = {
  agentId: string
  kind: 'device' | 'storage'
  target: string
  status: 'granted' | 'read' | 'readWrite' | 'denied'
}

/** A storage without a row asks first. */
export type StorageChoice = 'ask' | 'read' | 'readWrite' | 'denied'

export type AgentFiles = {
  agentId: string
  device: boolean
  storages: { id: string; name: string; status: StorageChoice }[]
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
    }
  })
}
