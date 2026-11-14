// The link a pinned shortcut of a pass opens (spec 092, FR-038): `haex-vault://pass/<id>`. The id is
// an opaque UUID and nothing else is in it, so no code and no title can travel in a link. Pure, so
// `scripts/check-wallet-passlink.ts` runs it without vue. Rust agrees (`pass_shortcut.rs`).

/** What a link to a pass starts with. */
export const PASS_LINK_PREFIX = 'haex-vault://pass/'

/** A UUID in its usual hyphenated form. */
const UUID_FORM =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/** The id a link to a pass names (lower case), or `null` for anything else. */
export function passIdFromLink(url: unknown): string | null {
  if (typeof url !== 'string' || !url.startsWith(PASS_LINK_PREFIX)) return null
  const id = url.slice(PASS_LINK_PREFIX.length)
  return UUID_FORM.test(id) ? id.toLowerCase() : null
}
