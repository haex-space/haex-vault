// The pure side of the per-vault CLI tools (spec 094): what a status says in words and numbers.
// Kept free of Vue and Tauri so `scripts/check-cli-tools.ts` can load it with Node type stripping.

// The parts of the generated `CliToolStatus` and `CliToolProgress` this module reads, written out
// here so that a script that loads it with Node type stripping needs no binding with imports of
// its own.
export type CliToolState =
  | 'not_installed'
  | 'installed'
  | 'installing'
  | 'needs_reinstall'
  | 'unsupported'

export type CliToolIsolation = 'best_effort' | 'enforced'

interface CliToolStatus {
  state: CliToolState
  reason: string | null
  isolation: CliToolIsolation
}

interface CliToolProgress {
  phase: 'downloading' | 'verifying' | 'unpacking' | 'publishing'
  bytesDone: number
  bytesTotal: number
}

/** The tools the Settings page lists, in the order of the catalog. */
export type CliToolVendor = 'claude' | 'codex'

/** The install is offered in these two states, and only in these. */
export function installable(status: Pick<CliToolStatus, 'state'>): boolean {
  return status.state === 'not_installed' || status.state === 'needs_reinstall'
}

/** An install of this tool runs in this vault. */
export function installing(status: Pick<CliToolStatus, 'state'>): boolean {
  return status.state === 'installing'
}

/** 0 to 1 while a download has a known size, `null` while it has none (an indeterminate bar). */
export function progressFraction(
  progress:
    Pick<CliToolProgress, 'phase' | 'bytesDone' | 'bytesTotal'> | undefined,
): number | null {
  if (
    !progress ||
    progress.phase !== 'downloading' ||
    progress.bytesTotal <= 0
  ) {
    return null
  }
  return Math.min(1, Math.max(0, progress.bytesDone / progress.bytesTotal))
}

/** `294 MB`, `1.2 GB`: decimal units, as the download hosts and file managers count them. */
export function formatBytes(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`
  return `${Math.round(bytes / 1e6)} MB`
}

/** The i18n key of the words that name the state of a tool. */
export function stateKey(
  status: Pick<CliToolStatus, 'state' | 'reason'>,
): string {
  if (status.state === 'needs_reinstall') {
    return status.reason === 'platform'
      ? 'settings.cliTools.state.needsReinstallPlatform'
      : 'settings.cliTools.state.needsReinstallIncomplete'
  }
  const keys: Record<CliToolState, string> = {
    not_installed: 'settings.cliTools.state.notInstalled',
    installed: 'settings.cliTools.state.installed',
    installing: 'settings.cliTools.state.installing',
    needs_reinstall: 'settings.cliTools.state.needsReinstallIncomplete',
    unsupported: 'settings.cliTools.state.unsupported',
  }
  return keys[status.state]
}

/** The i18n key of the words that say how far a tool's processes are kept inside the vault. */
export function isolationKey(status: Pick<CliToolStatus, 'isolation'>): string {
  return status.isolation === 'enforced'
    ? 'settings.cliTools.isolation.enforced'
    : 'settings.cliTools.isolation.bestEffort'
}

/** The i18n key of the words that name the phase of a running install. */
export function phaseKey(
  progress: Pick<CliToolProgress, 'phase'> | undefined,
): string {
  return `settings.cliTools.phase.${progress?.phase ?? 'downloading'}`
}

/**
 * The reasons an adapter gives when it cannot start a tool (`not installed in this vault: <vendor>`,
 * `needs reinstall: <vendor>`) and the vendor they name; `null` for any other text. The chat shows
 * these as an invitation to install, not as a failure.
 */
export function unavailableTool(
  text: string,
): { kind: 'not_installed' | 'needs_reinstall'; vendor: string } | null {
  const match =
    /(not installed in this vault|needs reinstall): ([a-z0-9]+)/.exec(text)
  if (!match) return null
  return {
    kind: match[1] === 'needs reinstall' ? 'needs_reinstall' : 'not_installed',
    vendor: match[2]!,
  }
}

/** The stable code of a failed tool action (`cli_tools.<name>`) in an error text; `null` if none. */
export function cliToolErrorCode(text: string): string | null {
  return /cli_tools\.[a-z_]+/.exec(text)?.[0] ?? null
}

/** The picker entry of a delegate whose tool the vault does not have; choosing it opens the install. */
export function isNotInstalledEntry(modelId: string): boolean {
  return /^delegate-[a-z0-9]+:not-installed$/.test(modelId)
}

/**
 * Whether a failed install is the user's own cancel, which is not reported. The error is the raw
 * one the command rejected with (`{ kind: 'InvalidInput', reason }`), not its translation.
 */
export function isCancelledInstall(error: unknown): boolean {
  const reason =
    typeof error === 'string'
      ? error
      : (error as { reason?: unknown } | null)?.reason
  return typeof reason === 'string' && reason.includes('cli_tools.cancelled')
}
