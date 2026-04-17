import { z } from 'zod'

// Schema factory that takes i18n translate function
export const createConnectWizardSchema = (t: (key: string) => string) => ({
  originUrl: z
    .string()
    .min(1, { message: t('validation.originUrlRequired') })
    .url({ message: t('validation.originUrlInvalid') }),
  vaultName: z
    .string()
    .min(1, { message: t('validation.vaultNameRequired') })
    .max(255, { message: t('validation.vaultNameTooLong') }),
  vaultPassword: z
    .string()
    .min(6, { message: t('validation.vaultPasswordMinLength') })
    .max(255, { message: t('validation.vaultPasswordTooLong') }),
})
