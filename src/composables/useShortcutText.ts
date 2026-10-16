import { detectPlatform } from '~/lib/wm/keybindings'
import { shortcutLabel, type ShortcutHint } from '~/lib/wm/shortcutLabel'

/** The shortcut next to a menu action (spec 036 FR-016, spec 044 FR-017b), with the key names of
 * the interface language on Linux and Windows and the symbols on macOS. */
export function useShortcutText() {
  const { t } = useI18n()
  const platform = detectPlatform(
    navigator as Navigator & { userAgentData?: { platform?: string } },
  )

  function shortcut(hint: ShortcutHint | undefined): string {
    if (!hint) return ''
    return shortcutLabel(hint, platform, {
      mod: t('wm.keys.mod'),
      shift: t('wm.keys.shift'),
      Delete: t('wm.keys.delete'),
      Enter: t('wm.keys.enter'),
    })
  }

  return { platform, shortcut }
}
