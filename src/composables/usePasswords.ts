import { invoke } from '@tauri-apps/api/core'
import type { CopyField } from '@bindings/CopyField'
import type { CopyOptions } from '@bindings/CopyOptions'
import type { CopyReport } from '@bindings/CopyReport'
import type { CopyResult } from '@bindings/CopyResult'
import type { CopyTextArgs } from '@bindings/CopyTextArgs'
import type { TextContent } from '@bindings/TextContent'
import type { TextField } from '@bindings/TextField'
import type { CreateGroupResult } from '@bindings/CreateGroupResult'
import type { CreateItemResult } from '@bindings/CreateItemResult'
import type { GroupPatch } from '@bindings/GroupPatch'
import type { ItemDetail } from '@bindings/ItemDetail'
import type { ItemInput } from '@bindings/ItemInput'
import type { ItemPatch } from '@bindings/ItemPatch'
import type { OpenSystemOutcome } from '@bindings/OpenSystemOutcome'
import type { PickedFile } from '@bindings/PickedFile'
import type { Preset } from '@bindings/Preset'
import type { RefMark } from '@bindings/RefMark'
import type { RefMarkKind } from '@bindings/RefMarkKind'
import type { ReferenceUsage } from '@bindings/ReferenceUsage'
import type { PresetInput } from '@bindings/PresetInput'
import type { PresetSaveResult } from '@bindings/PresetSaveResult'
import type { MoveResult } from '@bindings/MoveResult'
import type { ImportArgs } from '@bindings/ImportArgs'
import type { ImportPreview } from '@bindings/ImportPreview'
import type { ImportReport } from '@bindings/ImportReport'
import type { ImportRunArgs } from '@bindings/ImportRunArgs'
import type { AttachmentView } from '@bindings/AttachmentView'
import type { AffectedResult } from '@bindings/AffectedResult'
import type { HistorySecret } from '@bindings/HistorySecret'
import type { ItemUsageResult } from '@bindings/ItemUsageResult'
import type { Overview } from '@bindings/Overview'
import type { RestoreOutcome } from '@bindings/RestoreOutcome'
import type { SnapshotHeader } from '@bindings/SnapshotHeader'
import type { SnapshotView } from '@bindings/SnapshotView'
import type { RevealedSecret } from '@bindings/RevealedSecret'
import type { SecretField } from '@bindings/SecretField'
import type { SetTagsResult } from '@bindings/SetTagsResult'
import type { Target } from '@bindings/Target'
import type { TotpCode } from '@bindings/TotpCode'
import type { UpdateItemResult } from '@bindings/UpdateItemResult'

/**
 * Thin `invoke` wrappers of the password manager commands (spec 034,
 * `contracts/tauri-commands.md`). Arguments travel in one `args` object; the backend decides who the
 * caller is, never the frontend. A secret comes back only from `revealAsync`; copying and the TOTP
 * code never hand it over.
 */
/** Where a copied text comes from: its placeholders resolve as in this field of this entry. */
export interface CopyTextSource {
  /** `null` for a new entry. */
  itemId: string | null
  field: TextField
}

export function usePasswords() {
  const loadOverviewAsync = () => invoke<Overview>('passwords_load_overview')

  const getItemAsync = (itemId: string) =>
    invoke<ItemDetail>('passwords_get_item', { args: { itemId } })

  const revealAsync = (itemId: string, field: SecretField) =>
    invoke<RevealedSecret>('passwords_reveal', { args: { itemId, field } })

  const totpCodeAsync = (itemId: string) =>
    invoke<TotpCode>('passwords_totp_code', { args: { itemId } })

  const copyFieldAsync = (itemId: string, field: CopyField) =>
    invoke<CopyResult>('passwords_copy_field', { args: { itemId, field } })

  /** A text the window holds already (an editor value), with the clearing of every other copy.
   * With `from`, Rust resolves its placeholders as in that field of the entry first (FR-045). */
  const copyTextAsync = (text: string, from?: CopyTextSource) =>
    invoke<CopyResult>('passwords_copy_text', {
      args: {
        text,
        itemId: from?.itemId ?? undefined,
        field: from?.field,
      } satisfies CopyTextArgs,
    })

  const createItemAsync = (input: ItemInput, groupId?: string) =>
    invoke<CreateItemResult>('passwords_create_item', {
      args: { input, groupId },
    })

  const updateItemAsync = (
    itemId: string,
    expectedUpdatedAt: string,
    patch: ItemPatch,
  ) =>
    invoke<UpdateItemResult>('passwords_update_item', {
      args: { itemId, expectedUpdatedAt, patch },
    })

  const renamePasskeyAsync = (passkeyId: string, nickname: string | null) =>
    invoke<null>('passwords_passkey_rename', { args: { passkeyId, nickname } })

  const deletePasskeyAsync = (passkeyId: string) =>
    invoke<null>('passwords_passkey_delete', { args: { passkeyId } })

  /** "Verweis lösen": drops the link that shows another entry's passkey here. */
  const passkeyUnlinkAsync = (itemId: string, passkeyId: string) =>
    invoke<null>('passwords_passkey_unlink', { args: { itemId, passkeyId } })

  // Files travel as the choice of the system's dialogs (spec 043: a path or an address), never as
  // bytes (FR-019).
  const attachmentAddAsync = (itemId: string, file: PickedFile) =>
    invoke<AttachmentView>('passwords_attachment_add', {
      args: { itemId, file },
    })
  const attachmentRenameAsync = (attachmentId: string, fileName: string) =>
    invoke<string>('passwords_attachment_rename', {
      args: { attachmentId, fileName },
    })
  const attachmentRemoveAsync = (attachmentId: string) =>
    invoke<null>('passwords_attachment_remove', { args: { attachmentId } })
  const attachmentSaveAsync = (attachmentId: string, file: PickedFile) =>
    invoke<null>('passwords_attachment_save', { args: { attachmentId, file } })
  const importPreviewAsync = (args: ImportArgs) =>
    invoke<ImportPreview>('passwords_import_preview', { args })
  const importRunAsync = (args: ImportRunArgs) =>
    invoke<ImportReport>('passwords_import_run', { args })
  const importCancelAsync = () => invoke<null>('passwords_import_cancel')
  const importReportSaveAsync = (report: ImportReport, file: PickedFile) =>
    invoke<null>('passwords_import_report_save', { args: { report, file } })
  const attachmentPreviewAsync = (attachmentId: string) =>
    invoke<ArrayBuffer>('passwords_attachment_preview', {
      args: { attachmentId },
    })
  const attachmentTextAsync = (attachmentId: string) =>
    invoke<TextContent>('passwords_attachment_text', { args: { attachmentId } })
  /** A PDF attachment as a URL of the media server for `tabId` (spec 058); release it with
   * `files_release`. */
  const attachmentOpenAsync = (attachmentId: string, tabId: string) =>
    invoke<string>('passwords_attachment_open', {
      args: { attachmentId, tabId },
    })
  /** Opens an attachment with the system's app (spec 058); `needsNotice` until the user confirmed
   * the notice about the decrypted copy, or chose "Nicht mehr fragen" on this device. */
  const attachmentOpenSystemAsync = (
    attachmentId: string,
    confirmed = false,
    dontAskAgain = false,
  ) =>
    invoke<OpenSystemOutcome>('passwords_attachment_open_system', {
      args: { attachmentId, confirmed, dontAskAgain },
    })

  const createGroupAsync = (args: {
    name: string
    description?: string
    icon?: string
    color?: string
    parentId?: string
  }) => invoke<CreateGroupResult>('passwords_create_group', { args })

  const updateGroupAsync = (groupId: string, patch: GroupPatch) =>
    invoke<null>('passwords_update_group', { args: { groupId, patch } })

  const reorderGroupsAsync = (parentId: string | null, orderedIds: string[]) =>
    invoke<null>('passwords_reorder_groups', {
      args: { parentId, orderedIds },
    })

  const moveAsync = (targets: Target[], toGroupId: string | null) =>
    invoke<MoveResult>('passwords_move', { args: { targets, toGroupId } })

  const setTagsAsync = (itemIds: string[], add: string[], remove: string[]) =>
    invoke<SetTagsResult>('passwords_set_tags', {
      args: { itemIds, add, remove },
    })

  const renameTagAsync = (tagId: string, name: string) =>
    invoke<null>('passwords_rename_tag', { args: { tagId, name } })

  const setTagColorAsync = (tagId: string, color: string | null) =>
    invoke<null>('passwords_set_tag_color', { args: { tagId, color } })

  const deleteTagAsync = (tagId: string) =>
    invoke<null>('passwords_delete_tag', { args: { tagId } })

  const presetListAsync = () => invoke<Preset[]>('passwords_preset_list')

  const presetSaveAsync = (preset: PresetInput) =>
    invoke<PresetSaveResult>('passwords_preset_save', { args: { preset } })

  const presetDeleteAsync = (presetId: string) =>
    invoke<null>('passwords_preset_delete', { args: { presetId } })

  const trashAsync = (targets: Target[]) =>
    invoke<AffectedResult>('passwords_trash', { args: { targets } })

  const restoreAsync = (targets: Target[]) =>
    invoke<AffectedResult>('passwords_restore', { args: { targets } })

  /** With `inlineReferences` the placeholders on the deleted entries become own values first
   * (spec 036, FR-048). */
  const deletePermanentlyAsync = (
    targets: Target[],
    inlineReferences = false,
  ) =>
    invoke<AffectedResult>('passwords_delete_permanently', {
      args: { targets, inlineReferences },
    })

  const emptyTrashAsync = (inlineReferences = false) =>
    invoke<AffectedResult>('passwords_empty_trash', {
      args: { inlineReferences },
    })

  // Spec 036, references (contracts/tauri-commands.md): the window never parses or builds a
  // placeholder itself.
  const referencesParseAsync = (text: string) =>
    invoke<RefMark[]>('passwords_references_parse', { args: { text } })

  const referenceTokenAsync = (
    itemId: string,
    kind: RefMarkKind,
    key?: string,
  ) =>
    invoke<string>('passwords_reference_token', {
      args: key === undefined ? { itemId, kind } : { itemId, kind, key },
    })

  const itemKeyNamesAsync = (itemId: string) =>
    invoke<string[]>('passwords_item_key_names', { args: { itemId } })

  const referenceUsageAsync = (itemIds: string[]) =>
    invoke<ReferenceUsage[]>('passwords_reference_usage', {
      args: { itemIds },
    })

  /** Copies entries and folders in one transaction (spec 036, FR-015). */
  const copyAsync = (
    targets: Target[],
    intoGroupId: string | null,
    options: CopyOptions,
  ) =>
    invoke<CopyReport>('passwords_copy', {
      args: { targets, intoGroupId, options },
    })

  const itemUsageAsync = (itemId: string) =>
    invoke<ItemUsageResult>('passwords_item_usage', { args: { itemId } })

  const historyListAsync = (itemId: string) =>
    invoke<SnapshotHeader[]>('passwords_history_list', { args: { itemId } })

  const historyGetAsync = (snapshotId: string) =>
    invoke<SnapshotView>('passwords_history_get', { args: { snapshotId } })

  const historyRevealAsync = (snapshotId: string, field: HistorySecret) =>
    invoke<RevealedSecret>('passwords_history_reveal', {
      args: { snapshotId, field },
    })

  const historyCopyAsync = (snapshotId: string, field: HistorySecret) =>
    invoke<CopyResult>('passwords_history_copy', {
      args: { snapshotId, field },
    })

  const historyRestoreAsync = (
    itemId: string,
    snapshotId: string,
    expectedUpdatedAt: string,
  ) =>
    invoke<RestoreOutcome>('passwords_history_restore', {
      args: { itemId, snapshotId, expectedUpdatedAt },
    })

  return {
    referencesParseAsync,
    referenceTokenAsync,
    itemKeyNamesAsync,
    referenceUsageAsync,
    copyAsync,
    trashAsync,
    restoreAsync,
    deletePermanentlyAsync,
    emptyTrashAsync,
    itemUsageAsync,
    historyListAsync,
    historyGetAsync,
    historyRevealAsync,
    historyCopyAsync,
    historyRestoreAsync,
    presetListAsync,
    presetSaveAsync,
    presetDeleteAsync,
    createGroupAsync,
    updateGroupAsync,
    reorderGroupsAsync,
    moveAsync,
    setTagsAsync,
    renameTagAsync,
    setTagColorAsync,
    deleteTagAsync,
    loadOverviewAsync,
    getItemAsync,
    revealAsync,
    totpCodeAsync,
    copyFieldAsync,
    copyTextAsync,
    createItemAsync,
    updateItemAsync,
    renamePasskeyAsync,
    deletePasskeyAsync,
    passkeyUnlinkAsync,
    attachmentAddAsync,
    attachmentRenameAsync,
    attachmentRemoveAsync,
    attachmentSaveAsync,
    attachmentPreviewAsync,
    attachmentTextAsync,
    attachmentOpenAsync,
    attachmentOpenSystemAsync,
    importPreviewAsync,
    importRunAsync,
    importCancelAsync,
    importReportSaveAsync,
  }
}
