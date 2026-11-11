// An OpenAI-compatible model server (spec 055 US5, T056): the check of its address in the form, and
// what the user states about one of its models, which its `/models` answer does not say. Pure:
// `components/settings/ModelServersView.vue` and `ModelServerView.vue` use it,
// `scripts/check-provider-address.ts` tests it.

export const OPENAI_COMPATIBLE = 'openai_compatible'

export type AddressProblem =
  'empty' | 'invalid' | 'scheme' | 'credentials' | 'query'

/** What is wrong with a server's address, or `null`. The address goes up to the API's root, such
 * as `http://192.168.1.20:11434/v1`; a key never goes into it. */
export function addressProblem(text: string): AddressProblem | null {
  const trimmed = text.trim()
  if (trimmed === '') return 'empty'
  // `192.168.1.20:11434` or `server/v1` lacks the scheme, the most common slip.
  if (!/^[a-z][a-z0-9+.-]*:/i.test(trimmed) || /^[^:]+:\d/.test(trimmed))
    return 'scheme'
  let url: URL
  try {
    url = new URL(trimmed)
  } catch {
    return 'invalid'
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') return 'scheme'
  if (url.hostname === '') return 'invalid'
  if (url.username !== '' || url.password !== '') return 'credentials'
  if (url.search !== '' || url.hash !== '') return 'query'
  return null
}

/** What the user stated about a model, as Rust stores it; `null` = not stated. */
export interface UserStatement {
  acceptsImages: boolean | null
  effortLevels: string[] | null
}

export type ImagesChoice = 'unstated' | 'yes' | 'no'

export function imagesChoice(user?: UserStatement | null): ImagesChoice {
  if (user?.acceptsImages === true) return 'yes'
  if (user?.acceptsImages === false) return 'no'
  return 'unstated'
}

export function acceptsImagesOf(choice: ImagesChoice): boolean | null {
  return choice === 'unstated' ? null : choice === 'yes'
}

/** The effort levels as the field shows them. */
export function levelsText(levels?: string[] | null): string {
  return levels?.join(', ') ?? ''
}

/** The levels a field holds, lowest first, separated by commas or blanks; an empty field states
 * nothing. */
export function parseLevels(text: string): string[] | null {
  const levels = text.split(/[\s,]+/).filter((level) => level !== '')
  return levels.length === 0 ? null : levels
}

/** What a server's model list says a model produces (spec 012 FR-024); `null`/absent: it says
 * nothing, which is an ordinary chat model. */
export type ModelKind = 'text' | 'image' | 'audio' | 'other'

/** The part of a model's capability record these helpers read. */
export interface DescribedCapabilities {
  kind?: ModelKind | null
  acceptedAttachmentKinds?: string[] | null
  reasoning?: { kind: string } | null
  user?: UserStatement | null
}

export const KIND_ORDER: ModelKind[] = ['text', 'image', 'audio', 'other']

/** The effort levels the field offers for a model that reasons: what OpenAI-style servers take. */
export const LEVEL_SUGGESTION = ['low', 'medium', 'high']

export function kindOf(capabilities?: DescribedCapabilities | null): ModelKind {
  return capabilities?.kind ?? 'text'
}

/** The models by kind, in a fixed order, without empty groups. */
export function groupByKind<
  T extends { capabilities?: DescribedCapabilities | null },
>(models: T[]): { kind: ModelKind; models: T[] }[] {
  return KIND_ORDER.map((kind) => ({
    kind,
    models: models.filter((m) => kindOf(m.capabilities) === kind),
  })).filter((group) => group.models.length > 0)
}

/** Whether the server itself says if a model takes images (`true`/`false`), or `null` when it does
 * not, or when the user's statement stands in for it. A reported value needs no choice. */
export function imagesReported(
  capabilities?: DescribedCapabilities | null,
): boolean | null {
  if (capabilities?.user?.acceptsImages != null) return null
  const kinds = capabilities?.acceptedAttachmentKinds
  return kinds == null ? null : kinds.includes('image')
}

/** Whether a model's effort field is shown: the model is known to reason, or the server describes
 * nothing about its models, or the user has stated levels already. */
export function showsLevels(
  capabilities?: DescribedCapabilities | null,
): boolean {
  if (capabilities?.user?.effortLevels != null) return true
  const reasoning = capabilities?.reasoning?.kind
  if (reasoning === 'model_managed' || reasoning === 'presets') return true
  return reasoning == null && capabilities?.kind == null
}

/** Whether the field offers [`LEVEL_SUGGESTION`]: a reasoning model whose levels nobody stated. */
export function suggestsLevels(
  capabilities?: DescribedCapabilities | null,
): boolean {
  return (
    capabilities?.reasoning?.kind === 'model_managed' &&
    capabilities.user?.effortLevels == null
  )
}
