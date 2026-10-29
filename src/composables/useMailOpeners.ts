import { computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import { mailTargetBehavior, openerRows } from '~/lib/mail/openers'
import { useExtensionsStore } from '~/stores/extensions'
import { useWindowManagerStore } from '~/stores/windowManager'

/**
 * The extensions that write mail and the hand-over to one of them (spec 063, US1): the mail button
 * offers the rows, a choice sends the link to holzi, which reads it again and parks the fields for
 * the extension; the extension's tab comes forward and takes them.
 */
export function useMailOpeners() {
  const { errString } = useErrorString()
  const extensions = useExtensionsStore()
  const wm = useWindowManagerStore()

  const rows = computed(() => openerRows(extensions.list))
  const behavior = computed(() => mailTargetBehavior(rows.value))

  /** Gives `link` to the extension and shows it; `false` when holzi refused (a message says why). */
  async function openWithAsync(
    extensionId: string,
    link: string,
  ): Promise<boolean> {
    try {
      await invoke('opener_mail_open', { extensionId, link })
    } catch (error) {
      toast.error(errString(error))
      return false
    }
    wm.showExtension(extensionId)
    return true
  }

  return { rows, behavior, openWithAsync }
}
