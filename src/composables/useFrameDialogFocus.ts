import { onMounted, type Ref } from 'vue'

/**
 * The keyboard of a dialog drawn over an extension's frame (`FrameDialog.vue`, `StorageDialog.vue`).
 * It takes the focus when it opens: `autofocus` only works while a document loads, and the keyboard
 * would otherwise stay in the frame below, where Enter asks the same question again. It keeps the
 * keyboard while it is open, since Escape and Enter answer it only from inside: focus that falls to
 * nothing or to the inert frame comes back, focus moved to another part of holzi stays there.
 *
 * Returns the handler for the dialog's `focusout`.
 */
export function useFrameDialogFocus(
  root: Ref<HTMLElement | null>,
  confirmTestId: string,
): () => void {
  function focusConfirm(): void {
    root.value
      ?.querySelector<HTMLElement>(`[data-testid="${confirmTestId}"]`)
      ?.focus()
  }

  /** Takes the keyboard back after the next render if it fell out of the dialog to nowhere. */
  function reclaimFocus(): void {
    requestAnimationFrame(() => {
      const active = document.activeElement
      if (!root.value || root.value.contains(active)) return
      if (
        active === null ||
        active === document.body ||
        active.closest('[inert]')
      )
        focusConfirm()
    })
  }

  onMounted(() => {
    focusConfirm()
    // The frame below turns inert in the same update and can keep the keyboard until it renders.
    reclaimFocus()
  })

  return reclaimFocus
}
