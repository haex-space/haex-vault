// Errors show as toasts (operator rule 2026-11-03), so a scenario that checks a failure reads the
// error toasts on screen instead of an inline alert.
import type { FlowInstance } from './flows.ts'

/** Selects the error toasts of `vue-sonner` (`ShadcnSonnerToaster` in `app.vue`). */
export const ERROR_TOAST = '[data-sonner-toast][data-type="error"]'

/** The texts of the error toasts on screen, oldest first. */
export const errorToasts = (page: Pick<FlowInstance, 'exec'>) =>
  page.exec<string[]>(
    `return [...document.querySelectorAll(arguments[0])].map((toast) => toast.textContent || '')`,
    [ERROR_TOAST],
  )
