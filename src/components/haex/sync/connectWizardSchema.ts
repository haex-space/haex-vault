import { z } from 'zod'

// Schema factory that takes i18n translate function
export const createConnectWizardSchema = (t: (key: string) => string) => ({
  serverUrl: z
    .string()
    .min(1, { message: t('validation.serverUrlRequired') })
    .url({ message: t('validation.serverUrlInvalid') }),
  email: z
    .string()
    .min(1, { message: t('validation.emailRequired') })
    .email({ message: t('validation.emailInvalid') }),
  password: z
    .string()
    .min(1, { message: t('validation.passwordRequired') }),
  vaultName: z
    .string()
    .min(1, { message: t('validation.vaultNameRequired') })
    .max(255, { message: t('validation.vaultNameTooLong') }),
  vaultPassword: z
    .string()
    .min(6, { message: t('validation.vaultPasswordMinLength') })
    .max(255, { message: t('validation.vaultPasswordTooLong') }),
})
