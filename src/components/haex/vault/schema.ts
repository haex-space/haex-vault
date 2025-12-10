import { z } from 'zod'

export const vaultSchema = {
  password: z
    .string()
    .min(6, { message: 'Password must be at least 6 characters' })
    .max(255),
  name: z
    .string()
    .min(1, { message: 'Name is required' })
    .max(255),
  path: z.string().min(4).endsWith('.db'),
}

// Schema factory for password change form with i18n support
export const createChangePasswordSchema = (t: (key: string) => string) =>
  z
    .object({
      currentPassword: z
        .string()
        .min(1, { message: t('password.errors.currentRequired') }),
      newPassword: z
        .string()
        .min(6, { message: t('password.errors.minLength') })
        .max(255),
      confirmPassword: z
        .string()
        .min(1, { message: t('password.errors.confirmRequired') }),
    })
    .refine((data) => data.newPassword === data.confirmPassword, {
      message: t('password.errors.mismatch'),
      path: ['confirmPassword'],
    })
