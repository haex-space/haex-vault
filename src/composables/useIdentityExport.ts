import { save } from '@tauri-apps/plugin-dialog'
import { writeFile } from '@tauri-apps/plugin-fs'
import type { SelectHaexIdentities } from '~/database/schemas'

export interface ExportOptions {
  selectedClaimIds: Set<string>
  includeAvatar: boolean
  includePrivateKey: boolean
}

export interface ExportClaim {
  id: string
  type: string
  value: string
}

export interface ExportOutcome {
  /** True when the user picked a path and the file was written. */
  saved: boolean
}

/**
 * Builds the identity-export JSON payload and writes it to a
 * user-selected file. Returns `{ saved: false }` when the user cancels
 * the native save dialog — consumers should treat that as a no-op, not
 * an error.
 *
 * Throws on filesystem write errors — consumer decides how to surface them.
 */
export function useIdentityExport() {
  const buildPayload = (
    identity: SelectHaexIdentities,
    claims: ExportClaim[],
    options: ExportOptions,
  ): Record<string, unknown> => {
    const selectedClaims = claims
      .filter((c) => options.selectedClaimIds.has(c.id))
      .map((c) => ({ type: c.type, value: c.value }))

    const payload: Record<string, unknown> = {
      did: identity.did,
      name: identity.name,
      claims: selectedClaims,
    }

    if (options.includeAvatar && identity.avatar) {
      payload.avatar = identity.avatar
    }

    if (options.includePrivateKey) {
      payload.privateKey = identity.privateKey
    }

    return payload
  }

  const exportToFileAsync = async (
    identity: SelectHaexIdentities,
    claims: ExportClaim[],
    options: ExportOptions,
    dialogTitle: string,
  ): Promise<ExportOutcome> => {
    const payload = buildPayload(identity, claims, options)
    const json = JSON.stringify(payload, null, 2)
    const data = new TextEncoder().encode(json)

    const filePath = await save({
      title: dialogTitle,
      defaultPath: `${identity.name.replace(/[^a-zA-Z0-9_-]/g, '_')}.identity.json`,
      filters: [{ name: 'JSON', extensions: ['json'] }],
    })
    if (!filePath) return { saved: false }

    await writeFile(filePath, data)
    return { saved: true }
  }

  return { buildPayload, exportToFileAsync }
}
