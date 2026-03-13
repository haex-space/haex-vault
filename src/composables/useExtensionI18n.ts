import type { ManifestI18nEntry } from '~~/src-tauri/bindings/ManifestI18nEntry'

type I18nMap = { [key in string]: ManifestI18nEntry } | null | undefined

/**
 * Resolves a localized field from an extension's i18n map.
 * Fallback chain: i18n[locale] → i18n["en"] → defaultValue
 */
function resolveI18nField(
  i18n: I18nMap,
  field: keyof ManifestI18nEntry,
  locale: string,
  defaultValue: string,
): string {
  if (!i18n) return defaultValue
  return i18n[locale]?.[field] ?? i18n['en']?.[field] ?? defaultValue
}

/**
 * Composable that provides locale-aware extension field resolution.
 * Uses the current app locale from @nuxtjs/i18n.
 */
export function useExtensionI18n() {
  const { locale } = useI18n()

  const localizedName = (
    name: string,
    i18n: I18nMap,
  ) => resolveI18nField(i18n, 'name', locale.value, name)

  const localizedDescription = (
    description: string | null | undefined,
    i18n: I18nMap,
  ) => resolveI18nField(i18n, 'description', locale.value, description ?? '')

  return {
    localizedName,
    localizedDescription,
    resolveI18nField,
  }
}
