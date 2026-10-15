/**
 * Where the focus goes when the menu that added a value closes (spec 051). The menu would hand it
 * back to its button, taking it from the field the user already types in: a person quicker than
 * the menu's close, or a slow device where the close comes late. So a field of the form that holds
 * the focus keeps it, and only otherwise the new value's first field takes it.
 */
export function focusAfterMenu(
  active: { inForm: boolean; editable: boolean } | null,
): 'keep' | 'new-value' {
  return active?.inForm && active.editable ? 'keep' : 'new-value'
}
