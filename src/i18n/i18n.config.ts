import { assistantName } from '~/composables/useAssistantName'
import { withAssistantName } from '~/lib/assistant/name'

/**
 * Texts that mean the assistant by name carry a token (`lib/assistant/name.ts`); it becomes the
 * written form of the vault's name here, so no view passes the name to `t`. Reading the name inside
 * the hook makes every text that shows it follow a rename.
 */
export default defineI18nConfig(() => ({
  postTranslation: (translated) =>
    typeof translated === 'string'
      ? withAssistantName(translated, assistantName.value.written)
      : translated,
}))
