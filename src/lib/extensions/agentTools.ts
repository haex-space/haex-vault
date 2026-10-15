// The tools an extension offers to the agent (spec 018) among its declared permissions: each is a
// permission of the kind `agentTool` with its effect as the action and its name as the target. Pure:
// `components/extensions/InstallDialog.vue` and the settings show them,
// `scripts/check-extensions-agent-tools.ts` tests it.

export const AGENT_TOOL_KIND = 'agentTool'
/** The target of the user's switch row: every tool of the extension. */
export const ALL_TOOLS = '*'

/** A declared permission as Rust sends it; agent tools carry their titles. */
export interface PermissionRow {
  kind: string
  action: string
  target: string
  titleDe?: string
  titleEn?: string
  supported?: boolean
}

/** The tool's title in `locale` (German for `de`, English otherwise), its name without titles. */
export function toolTitle(row: PermissionRow, locale: string): string {
  const title = locale === 'de' ? row.titleDe : row.titleEn
  return title ?? row.target
}

/** The i18n key of an effect (`read`, `change`, `destructive`). */
export function effectLabelKey(effect: string): string {
  return `extensions.permissions.effect.${effect}`
}

/** The agent tools, sorted by title, apart from the other permissions, which keep their order. */
export function splitAgentTools<T extends PermissionRow>(
  rows: readonly T[],
  locale: string,
): { tools: T[]; others: T[] } {
  const tools = rows
    .filter((row) => row.kind === AGENT_TOOL_KIND && row.target !== ALL_TOOLS)
    .sort((a, b) =>
      toolTitle(a, locale).localeCompare(toolTitle(b, locale), locale),
    )
  const others = rows.filter((row) => row.kind !== AGENT_TOOL_KIND)
  return { tools, others }
}

/** "Für den Agenten verfügbar": off only while the switch row `agentTool / * / denied` exists. */
export function agentToolsAvailable(
  rows: readonly (PermissionRow & { status?: string })[],
): boolean {
  return !rows.some(
    (row) =>
      row.kind === AGENT_TOOL_KIND &&
      row.target === ALL_TOOLS &&
      row.status === 'denied',
  )
}
