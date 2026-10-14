import { customLabel, kindOf } from '~/lib/contacts/kinds'

/** How kinds, labels and origins of contacts read in the window (spec 051). */
export function useContactLabels() {
  const { t, te } = useI18n()

  /** The name of a kind; an unknown kind shows as it is stored. */
  function kindName(kind: string): string {
    return kindOf(kind) ? t(`contacts.kinds.${kind}`) : kind
  }

  /** The name of a label: a label of the user's own as typed, a fixed one translated. */
  function labelName(label: string): string {
    const own = customLabel(label)
    if (own !== null) return own
    return te(`contacts.labels.${label}`)
      ? t(`contacts.labels.${label}`)
      : label
  }

  return { kindName, labelName }
}
