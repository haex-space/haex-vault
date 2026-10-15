// How the chat names a call of an extension's tool (spec 018, research R8): by its extension and
// the tool's title, never by the model-facing name `x_<slug>_<tool>`. Pure, so the Node check
// scripts can load it (`scripts/check-chat-tool-origin.ts`).

/** Where an extension's tool comes from; Rust's `ToolOrigin`. */
export interface ToolOrigin {
  extensionId: string
  extensionName: string
  toolTitleDe: string
  toolTitleEn: string
  /** A development version of the extension. */
  dev: boolean
}

/** "<extension>: <tool title>", the title in German for `de`, in English otherwise. */
export function originLabel(origin: ToolOrigin, locale: string): string {
  const title = locale === 'de' ? origin.toolTitleDe : origin.toolTitleEn
  return `${origin.extensionName}: ${title}`
}

/** The fields of an input object with their values, strings as they are, others as JSON. */
export function inputRows(input: unknown): Array<[string, string]> {
  if (typeof input !== 'object' || input === null || Array.isArray(input))
    return []
  return Object.entries(input).map(([field, value]) => [
    field,
    typeof value === 'string' ? value : JSON.stringify(value),
  ])
}
