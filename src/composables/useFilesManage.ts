import { toast } from 'vue-sonner'
import type { Entry } from '@bindings/Entry'
import type { SourceRef } from '@bindings/SourceRef'
import type { TransferOp } from '@bindings/TransferOp'
import { asFilesError } from '~/composables/useFiles'
import {
  clickEntry,
  type ClickKeys,
  type DragPayload,
  dropOp,
  EMPTY_SELECTION,
  type FilesClipboard,
  menuSelection,
  pasteRefusal,
  pruneSelection,
  type Selection,
} from '~/lib/files/clipboard'
import { canCreateEncrypted } from '~/lib/files/encrypted'
import { entriesMenu, type FilesCommand, folderMenu } from '~/lib/files/menus'

/**
 * Managing files in a file browser tab (spec 044 US3): the selection, the context menu and the
 * action bar, holzi's clipboard, dragging inside holzi and from the system, the name and delete
 * dialogs, and the transfers they start (`stores/filesTransfers.ts`). `shown` is the folder as
 * listed, `reload` loads it again, `activate` opens an entry.
 */
export function useFilesManage(options: {
  source: Ref<SourceRef>
  path: Ref<string | null>
  shown: Ref<Entry[]>
  tabId: string
  reload: () => unknown
  activate: (entry: Entry) => void
}) {
  const { source, path, shown, tabId, reload, activate } = options
  const { t } = useI18n()
  const { statAsync, encryptedNoticeShownAsync, encryptedNoticeAckAsync } =
    useFiles()
  const transfers = useFilesTransfersStore()
  const { isAndroid } = useDeviceCapabilities()

  const selection = ref<Selection>(EMPTY_SELECTION)
  const selecting = computed(() => selection.value.paths.length > 0)
  watch(path, () => (selection.value = EMPTY_SELECTION))
  watch(shown, (entries) => {
    selection.value = pruneSelection(
      selection.value,
      entries.map((entry) => entry.path),
    )
  })

  /** The open folder is one of holzi's own places (FR-037): nothing changes in it. */
  const folderOwned = ref(false)
  /** The open folder is, or lies in, an encrypted folder (spec 048). */
  const folderEncrypted = ref(false)
  watch(
    [source, path],
    async ([current, folderPath]) => {
      folderOwned.value = false
      folderEncrypted.value = false
      if (folderPath === null) return
      const entry = await statAsync(current, folderPath).catch(() => null)
      if (path.value === folderPath) {
        folderOwned.value = entry?.holziOwned ?? false
        folderEncrypted.value = !!entry?.encrypted
      }
    },
    { immediate: true },
  )

  const entryAt = (target: string) =>
    shown.value.find((entry) => entry.path === target)

  function onPress(entry: Entry, keys: ClickKeys) {
    const result = clickEntry(
      selection.value,
      entry.path,
      shown.value.map((candidate) => candidate.path),
      keys,
    )
    selection.value = result.selection
    if (result.activate) activate(entry)
  }

  /** What the open context menu acts on: entries, or the folder's empty area (`null`). */
  const menuTargets = ref<Selection | null>(null)

  function onContext(entry: Entry | null) {
    menuTargets.value = entry
      ? menuSelection(selection.value, entry.path)
      : null
  }

  const canPaste = computed(() => transfers.clipboard !== null && !!path.value)

  function entriesOf(targets: Selection): Entry[] {
    return targets.paths.flatMap((target) => entryAt(target) ?? [])
  }

  const menuEntries = computed(() => {
    const targets = menuTargets.value
    if (!targets)
      return folderMenu({
        readOnly: folderOwned.value,
        canPaste: canPaste.value,
        canEncrypt: canCreateEncrypted({
          source: source.value,
          path: path.value,
          inEncrypted: folderEncrypted.value,
        }),
      })
    const entries = entriesOf(targets)
    return entriesMenu({
      count: entries.length,
      singleFile: entries.length === 1 && entries[0]?.kind === 'file',
      readOnly: folderOwned.value || entries.some((entry) => entry.holziOwned),
      selecting: selecting.value,
      systemApps: !isAndroid.value,
    })
  })

  const dimmed = computed(() =>
    transfers.clipboard?.op === 'cut'
      ? transfers.clipboard.items.map((item) => item.path)
      : [],
  )

  function errorText(cause: unknown): string {
    const code = asFilesError(cause)?.code
    return code ? t(`files.error.${code}`) : String(cause)
  }

  /** Starts a transfer; a refusal shows as a message. Whether it started. */
  async function startAsync(
    op: TransferOp,
    from: SourceRef,
    paths: string[],
    to: string | null,
  ): Promise<boolean> {
    try {
      await transfers.startAsync(
        op,
        from,
        paths,
        to === null ? null : { source: source.value, path: to },
        tabId,
        // A storage is not watched: its folder loads again once the transfer is through.
        () => void reload(),
      )
      return true
    } catch (cause) {
      toast.error(errorText(cause))
      return false
    }
  }

  /**
   * Copies or moves `clip` into the folder `target` (paste, or a drop inside holzi). Whether the
   * transfer started.
   */
  async function placeAsync(
    clip: FilesClipboard,
    target: string,
    targetOwned: boolean,
  ): Promise<boolean> {
    const refusal = pasteRefusal(clip.op, clip.folder, clip.items, {
      path: target,
      holziOwned: targetOwned,
    })
    if (refusal === 'nothing') return false
    if (refusal) {
      toast.error(t(`files.error.${refusal}`))
      return false
    }
    return await startAsync(
      clip.op === 'cut' ? 'move' : 'copy',
      clip.source as SourceRef,
      clip.items.map((item) => item.path),
      target,
    )
  }

  const naming = ref(false)
  const renaming = ref<Entry | null>(null)
  /** The name dialog creates an encrypted folder (spec 048). */
  const namingEncrypted = ref(false)
  /** The notice before the first encrypted folder (FR-002) is open. */
  const noticing = ref(false)

  function startEncrypted() {
    namingEncrypted.value = true
    naming.value = true
  }

  async function ackNotice() {
    await encryptedNoticeAckAsync().catch(() => {})
    startEncrypted()
  }
  const deleting = ref(false)
  const pendingDelete = ref<string[]>([])

  const {
    noticing: openNoticing,
    openAsync: openSystemAsync,
    confirmAsync: confirmOpenAsync,
  } = useFilesOpenSystem(source, (cause) => toast.error(errorText(cause)))

  function remember(op: FilesClipboard['op'], entries: Entry[]) {
    if (!path.value || !entries.length) return
    transfers.clipboard = {
      op,
      source: source.value,
      folder: path.value,
      items: entries.map((entry) => ({ path: entry.path, kind: entry.kind })),
    }
    selection.value = EMPTY_SELECTION
  }

  function deleteNow() {
    const paths = pendingDelete.value
    pendingDelete.value = []
    selection.value = EMPTY_SELECTION
    void startAsync('delete', source.value, paths, null)
  }

  /** Runs a command on `targets` (the context menu's, or the selection for the action bar). */
  async function run(command: FilesCommand, targets: Selection | null) {
    const entries = targets ? entriesOf(targets) : []
    const [first] = entries
    switch (command) {
      case 'open':
        if (first) activate(first)
        break
      case 'openSystem':
        if (first) await openSystemAsync(first.path)
        break
      case 'select':
        selection.value = {
          paths: [
            ...new Set([...selection.value.paths, ...(targets?.paths ?? [])]),
          ],
          anchor: targets?.anchor ?? null,
        }
        break
      case 'copy':
      case 'cut':
        remember(command, entries)
        break
      case 'paste': {
        const clip = transfers.clipboard
        if (!clip || !path.value) break
        // A refused cut stays on the clipboard, so it can go elsewhere.
        const started = await placeAsync(clip, path.value, folderOwned.value)
        if (started && clip.op === 'cut') transfers.clipboard = null
        break
      }
      case 'rename':
        if (first) {
          renaming.value = first
          naming.value = true
        }
        break
      case 'newFolder':
        renaming.value = null
        namingEncrypted.value = false
        naming.value = true
        break
      case 'newEncryptedFolder':
        renaming.value = null
        // Before the first encrypted folder on this device, the notice of FR-002.
        if (await encryptedNoticeShownAsync().catch(() => false))
          startEncrypted()
        else noticing.value = true
        break
      case 'delete':
        pendingDelete.value = entries.map((entry) => entry.path)
        // Desktops move to the trash; Android and storages delete for good and ask first (FR-023).
        if (isAndroid.value || source.value.kind === 'storage')
          deleting.value = true
        else deleteNow()
        break
    }
  }

  function runMenu(command: FilesCommand) {
    void run(command, menuTargets.value)
  }

  function runBar(command: FilesCommand) {
    void run(command, selection.value)
  }

  function onDrop(
    payload: DragPayload,
    target: string,
    keys: { ctrl: boolean; alt: boolean },
  ) {
    // Dropped onto itself.
    if (payload.items.some((item) => item.path === target)) return
    const sameSource =
      JSON.stringify(payload.source) === JSON.stringify(source.value)
    const op = dropOp(sameSource, {
      ...keys,
      mac: /Mac/.test(navigator.userAgent),
    })
    const owned =
      target === path.value
        ? folderOwned.value
        : (entryAt(target)?.holziOwned ?? false)
    selection.value = EMPTY_SELECTION
    void placeAsync({ ...payload, op }, target, owned)
  }

  async function importDropped(paths: string[]) {
    if (!path.value) return
    try {
      await transfers.importAsync(
        paths,
        { source: source.value, path: path.value },
        tabId,
      )
    } catch (cause) {
      toast.error(errorText(cause))
    }
  }

  function clearSelection() {
    selection.value = EMPTY_SELECTION
  }

  return {
    selection,
    selecting,
    clearSelection,
    folderOwned,
    canPaste,
    menuEntries,
    dimmed,
    errorText,
    onPress,
    onContext,
    runMenu,
    runBar,
    onDrop,
    importDropped,
    naming,
    renaming,
    namingEncrypted,
    noticing,
    ackNotice,
    deleting,
    pendingDelete,
    deleteNow,
    openNoticing,
    openSystemAsync,
    confirmOpenAsync,
  }
}
