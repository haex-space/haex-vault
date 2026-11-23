import { toast } from 'vue-sonner'

/**
 * What can be done with a file of a message that is on this device (spec 072 US3 scenario 4):
 * "Mit System-App öffnen" through the notice shared with the file browser and the password manager
 * (`useOpenSystemNotice`), and "Speichern unter" through the system's save dialog. The window that
 * shows the files renders the notice (`FilesOpenSystemNotice` on `noticing`).
 */
export function useChatFileActions() {
  const { t } = useI18n()
  const { errString } = useErrorString()
  const commands = useConversationCommands()
  const { pickSaveTargetAsync } = usePickedFile()

  /** The reasons of `chat_attachment_open_system` in words; anything else as usual. */
  function openSystemError(cause: unknown): string {
    const reason = (cause as { reason?: unknown } | null)?.reason
    if (reason === 'no_app') return t('chat.file.openSystemNoApp')
    return errString(cause)
  }

  const notice = useOpenSystemNotice(
    (attachmentId: string, confirmed, dontAskAgain) =>
      commands.openSystemAsync(attachmentId, confirmed, dontAskAgain),
    (cause) => toast.error(openSystemError(cause)),
  )

  async function saveAsync(file: { id: string; name: string | null }) {
    const target = await pickSaveTargetAsync(
      file.name ?? t('chat.file.unnamed'),
    )
    if (!target) return
    try {
      await commands.saveAsync(file.id, target)
      toast.success(t('chat.file.saved'))
    } catch (cause) {
      toast.error(errString(cause))
    }
  }

  return {
    noticing: notice.noticing,
    openSystem: notice.openAsync,
    confirmOpenSystem: notice.confirmAsync,
    saveAsync,
  }
}
