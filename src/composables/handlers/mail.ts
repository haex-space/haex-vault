import { TAURI_COMMANDS } from '@haex-space/vault-sdk'
import type {
  MailFetchRange,
  ImapConfig,
  MailMessage,
  MailboxInfo,
  MessageEnvelope,
  OutgoingMessage,
  SmtpConfig,
} from '@haex-space/vault-sdk'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { ExtensionRequest } from './types'
import { invokeWithPermissionPrompt } from './invoke'

/**
 * Iframe handler for IMAP fetch + SMTP send.
 *
 * Same pattern as `passwords.ts`: forwards `publicKey` + `name` so the
 * Rust side can resolve the extension and check `mail` permissions
 * (action `fetch` for IMAP ops, `send` for SMTP) against the relevant
 * mail-server hostname.
 */
export async function handleMailMethodAsync(
  request: ExtensionRequest,
  extension: IHaexSpaceExtension,
) {
  if (!extension || !request) {
    throw new Error('Extension not found')
  }

  const { method, params } = request
  const extInfo = {
    publicKey: extension.publicKey,
    name: extension.name,
  }

  switch (method) {
    case TAURI_COMMANDS.mail.listMailboxes: {
      const imap = params.imap as ImapConfig
      return invokeWithPermissionPrompt<MailboxInfo[]>(
        TAURI_COMMANDS.mail.listMailboxes,
        {
          imap,
          reference: params.reference as string | undefined,
          pattern: params.pattern as string | undefined,
          includeStatus: params.includeStatus as boolean | undefined,
          ...extInfo,
        },
      )
    }

    case TAURI_COMMANDS.mail.fetchEnvelopes: {
      const imap = params.imap as ImapConfig
      const mailbox = params.mailbox as string
      const range = params.range as MailFetchRange
      return invokeWithPermissionPrompt<MessageEnvelope[]>(
        TAURI_COMMANDS.mail.fetchEnvelopes,
        { imap, mailbox, range, ...extInfo },
      )
    }

    case TAURI_COMMANDS.mail.fetchMessage: {
      const imap = params.imap as ImapConfig
      const mailbox = params.mailbox as string
      const uid = params.uid as number
      return invokeWithPermissionPrompt<MailMessage>(
        TAURI_COMMANDS.mail.fetchMessage,
        { imap, mailbox, uid, ...extInfo },
      )
    }

    case TAURI_COMMANDS.mail.setFlags: {
      const imap = params.imap as ImapConfig
      const mailbox = params.mailbox as string
      const uids = params.uids as number[]
      const flags = params.flags as string[]
      const add = params.add as boolean
      return invokeWithPermissionPrompt<null>(
        TAURI_COMMANDS.mail.setFlags,
        { imap, mailbox, uids, flags, add, ...extInfo },
      )
    }

    case TAURI_COMMANDS.mail.moveMessages: {
      const imap = params.imap as ImapConfig
      const sourceMailbox = params.sourceMailbox as string
      const destinationMailbox = params.destinationMailbox as string
      const uids = params.uids as number[]
      return invokeWithPermissionPrompt<null>(
        TAURI_COMMANDS.mail.moveMessages,
        { imap, sourceMailbox, destinationMailbox, uids, ...extInfo },
      )
    }

    case TAURI_COMMANDS.mail.appendMessage: {
      const imap = params.imap as ImapConfig
      const mailbox = params.mailbox as string
      const rfc822Base64 = params.rfc822Base64 as string
      const flags = params.flags as string[] | undefined
      return invokeWithPermissionPrompt<null>(
        TAURI_COMMANDS.mail.appendMessage,
        { imap, mailbox, rfc822Base64, flags, ...extInfo },
      )
    }

    case TAURI_COMMANDS.mail.sendMessage: {
      const smtp = params.smtp as SmtpConfig
      const message = params.message as OutgoingMessage
      return invokeWithPermissionPrompt<string>(
        TAURI_COMMANDS.mail.sendMessage,
        { smtp, message, ...extInfo },
      )
    }

    case TAURI_COMMANDS.mail.buildRfc822: {
      const imapHost = params.imapHost as string
      const message = params.message as OutgoingMessage
      return invokeWithPermissionPrompt<string>(
        TAURI_COMMANDS.mail.buildRfc822,
        { imapHost, message, ...extInfo },
      )
    }

    default:
      throw new Error(`Unknown mail method: ${method}`)
  }
}
