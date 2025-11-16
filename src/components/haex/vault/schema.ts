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
