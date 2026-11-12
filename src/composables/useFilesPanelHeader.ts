import { computed, onBeforeUnmount, type ComputedRef, type Ref } from 'vue'
import type { FilesPanelContext } from '~/composables/useFilesSplit'
import type { FilesView } from '~/composables/useFilesPrefs'
import type { FilesHeaderCommand, PanelBarState } from '~/lib/files/header'
import { parentPath, type Sort } from '~/lib/files/state'

/**
 * One panel's bar for the window's header (spec 090-app-header-bar US4, FR-025): the state the
 * frame builds the header's controls from (`lib/files/header.ts`), and what to do for each command
 * the controls report. The commands run the same functions the panel's own bar (`PanelBar.vue`, used
 * with two panels) runs, so a control does the same in either place. Offered to the frame through
 * the panel context for as long as the panel lives; only the active panel's is shown.
 */
export function useFilesPanelHeader(options: {
  context: FilesPanelContext
  panelView: {
    view: Readonly<Ref<FilesView>>
    sort: Readonly<Ref<Sort>>
    setView(view: FilesView): void
    setSort(sort: Sort): void
  }
  path: Readonly<Ref<string | null>>
  title: Readonly<Ref<string | undefined>>
  storageName: Readonly<Ref<string | undefined>>
  readOnly: Readonly<Ref<boolean>>
  canEncrypt: Readonly<Ref<boolean>>
  go(path: string): void
  goUp(): void
  reload(): unknown
  runMenu(command: 'newFolder' | 'newFile' | 'newEncryptedFolder'): void
}) {
  const { context, panelView, path } = options
  const prefs = useFilesPrefs()

  const state: ComputedRef<PanelBarState> = computed(() => ({
    path: path.value,
    title: options.title.value,
    rootLabel: options.storageName.value,
    canGoUp: !!path.value && parentPath(path.value) !== null,
    canGoBack: context.canGoBack.value,
    canGoForward: context.canGoForward.value,
    view: panelView.view.value,
    sort: panelView.sort.value,
    showHidden: prefs.showHidden.value,
    canCreate: !options.readOnly.value && !!path.value,
    canCreateEncrypted: options.canEncrypt.value && !options.readOnly.value,
  }))

  function run(command: FilesHeaderCommand): void {
    switch (command.kind) {
      case 'back':
        return context.back()
      case 'forward':
        return context.forward()
      case 'up':
        return options.goUp()
      case 'reload':
        return void options.reload()
      case 'go':
        return options.go(command.path)
      case 'view':
        return panelView.setView(command.view)
      case 'sort':
        return panelView.setSort(command.sort)
      case 'hidden':
        return prefs.setShowHidden(command.show)
      case 'create':
        return options.runMenu(command.command)
      // The sidebar button and the split switch are the frame's.
      default:
        return
    }
  }

  onBeforeUnmount(context.registerHeader({ state, run }))
}
