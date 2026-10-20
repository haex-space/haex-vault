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
