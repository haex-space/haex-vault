// Errors show as toasts (operator rule 2026-11-03), and so does a success the UI shows nowhere
// else (2026-11-04), so a scenario reads the toasts on screen instead of an inline line.
import type { FlowInstance } from './flows.ts'

/** Selects the error toasts of `vue-sonner` (`ShadcnSonnerToaster` in `app.vue`). */
export const ERROR_TOAST = '[data-sonner-toast][data-type="error"]'

/** The texts of the error toasts on screen, oldest first. */
export const errorToasts = (page: Pick<FlowInstance, 'exec'>) =>
  page.exec<string[]>(
    `return [...document.querySelectorAll(arguments[0])].map((toast) => toast.textContent || '')`,
    [ERROR_TOAST],
  )

/** Selects the success toasts of `vue-sonner`. */
export const SUCCESS_TOAST = '[data-sonner-toast][data-type="success"]'

/** The texts of the success toasts on screen, oldest first. */
export const successToasts = (page: Pick<FlowInstance, 'exec'>) =>
  page.exec<string[]>(
    `return [...document.querySelectorAll(arguments[0])].map((toast) => toast.textContent || '')`,
    [SUCCESS_TOAST],
  )
