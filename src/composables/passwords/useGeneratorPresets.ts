import { and, eq, ne } from 'drizzle-orm'
import { haexPasswordsGeneratorPresets } from '~/database/schemas'
import type { SelectHaexPasswordsGeneratorPresets } from '~/database/schemas'
import { requireDb } from '~/stores/vault'

export type PasswordGeneratorPreset = SelectHaexPasswordsGeneratorPresets

export interface PasswordGeneratorPresetInput {
  name: string
  length: number
  uppercase: boolean
  lowercase: boolean
  numbers: boolean
  symbols: boolean
  excludeChars: string
  usePattern: boolean
  pattern: string
  isDefault?: boolean
}

export const usePasswordGeneratorPresets = () => {
  const getAllAsync = async (): Promise<PasswordGeneratorPreset[]> => {
    const db = requireDb()
    return await db.select().from(haexPasswordsGeneratorPresets)
  }

  const getDefaultAsync = async (): Promise<PasswordGeneratorPreset | null> => {
    const db = requireDb()
    const results = await db
      .select()
      .from(haexPasswordsGeneratorPresets)
      .where(eq(haexPasswordsGeneratorPresets.isDefault, true))
      .limit(1)
    return results[0] ?? null
  }

  // Unset isDefault on every other row — narrower than a WHERE-less update.
  const unsetOtherDefaultsAsync = async (keepId: string) => {
    const db = requireDb()
    await db
      .update(haexPasswordsGeneratorPresets)
      .set({ isDefault: false })
      .where(
        and(
          eq(haexPasswordsGeneratorPresets.isDefault, true),
          ne(haexPasswordsGeneratorPresets.id, keepId),
        ),
      )
  }

  const createAsync = async (
    preset: PasswordGeneratorPresetInput,
  ): Promise<string> => {
    const db = requireDb()
    const id = crypto.randomUUID()

    await db.insert(haexPasswordsGeneratorPresets).values({
      id,
      name: preset.name,
      length: preset.length,
      uppercase: preset.uppercase,
      lowercase: preset.lowercase,
      numbers: preset.numbers,
      symbols: preset.symbols,
      excludeChars: preset.excludeChars,
      usePattern: preset.usePattern,
      pattern: preset.pattern,
      isDefault: preset.isDefault ?? false,
    })

    if (preset.isDefault) await unsetOtherDefaultsAsync(id)
    return id
  }

  const updateAsync = async (
    id: string,
    preset: Partial<PasswordGeneratorPresetInput>,
  ): Promise<void> => {
    const db = requireDb()

    await db
      .update(haexPasswordsGeneratorPresets)
      .set(preset)
      .where(eq(haexPasswordsGeneratorPresets.id, id))

    if (preset.isDefault) await unsetOtherDefaultsAsync(id)
  }

  const deleteAsync = async (id: string): Promise<void> => {
    const db = requireDb()
    await db
      .delete(haexPasswordsGeneratorPresets)
      .where(eq(haexPasswordsGeneratorPresets.id, id))
  }

  return {
    getAllAsync,
    getDefaultAsync,
    createAsync,
    updateAsync,
    deleteAsync,
  }
}
