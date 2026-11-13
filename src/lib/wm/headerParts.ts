// Controls that more than one app's header description has (spec 093-app-headers, T019). Pure —
// relative `.ts` imports, so the Node check scripts load it.
import type { WmHeaderControl } from '../../types/bindings/WmHeaderControl.ts'

/**
 * The sidebar button: first in the header, pressed while the sidebar shows, and it never collapses
 * (`keep`), so the sidebar stays reachable however narrow the window is. The app passes the two
 * texts, so each key stays greppable in its own i18n group.
 */
export function sidebarToggle(options: {
  visible: boolean
  hideLabel: string
  showLabel: string
  testId: string
}): WmHeaderControl {
  return {
    type: 'toggle',
    id: 'sidebar',
    label: options.visible ? options.hideLabel : options.showLabel,
    icon: 'lucide:panel-left',
    pressed: options.visible,
    testId: options.testId,
    collapse: { form: 'keep' },
  }
}
