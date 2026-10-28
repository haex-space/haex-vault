/**
 * The drives of this device for every file browser frame (spec 064 FR-004, research R8): one
 * subscription while at least one frame is open; the drive monitor in Rust sends the whole list at
 * once and after every change. The list and the busy drives live at module level on purpose: they
 * are the same for every frame and survive a frame's remount.
 */
import type { Drive } from '@bindings/Drive'

const drives = ref<Drive[]>([])
const received = ref(false)
/** Drives with an action running (mounting), so the sidebar shows a spinner and takes no click. */
const busy = ref(new Set<string>())
let subscription: Promise<number | null> | null = null

export function useFilesDrives() {
  const { drivesWatchAsync, drivesUnwatchAsync } = useFiles()

  /** Subscribes once; the first frame calls it. */
  function start() {
    subscription ??= drivesWatchAsync((change) => {
      drives.value = change.drives
      received.value = true
    }).catch(() => null)
  }

  /** Ends the subscription; the last frame calls it. */
  function stop() {
    const pending = subscription
    subscription = null
    received.value = false
    drives.value = []
    void pending
      ?.then((id) => (id === null ? undefined : drivesUnwatchAsync(id)))
      .catch(() => {})
  }

  function setBusy(id: string, on: boolean) {
    const next = new Set(busy.value)
    if (on) next.add(id)
    else next.delete(id)
    busy.value = next
  }

  return {
    drives: readonly(drives),
    received: readonly(received),
    busy: readonly(busy),
    start,
    stop,
    setBusy,
  }
}
