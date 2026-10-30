<script setup lang="ts">
/**
 * Instead of the folder of a restored tab whose drive is plugged in but not mounted, or locked (spec
 * 064 FR-025, contracts/ui.md "Tab-Hinweis"): one button that opens the drive as a click in the
 * sidebar does. A locked one without a saved password that works goes to the unlock dialog of the
 * panel, which outlives this notice once the drive is open.
 */
import type { Drive } from '@bindings/Drive'
import type { DriveOpened } from '@bindings/DriveOpened'

const props = defineProps<{ drive: Drive }>()
const emit = defineEmits<{
  opened: [path: string]
  needsPassword: [answer: Extract<DriveOpened, { kind: 'needsPassword' }>]
}>()

const { t } = useI18n()
const { nameOf, openDriveAsync } = useFilesDriveActions()
const { busy } = useFilesDrives()

const locked = computed(() => props.drive.state === 'locked')
const name = computed(() => nameOf(props.drive))

async function onOpen() {
  const opened = await openDriveAsync(props.drive)
  if (opened?.kind === 'opened') emit('opened', opened.path)
  else if (opened?.kind === 'needsPassword') emit('needsPassword', opened)
}
</script>

<template>
  <div
    class="flex h-full flex-col items-center justify-center gap-3 p-6 text-center"
    data-testid="files-drive-notice"
  >
    <Icon
      :name="locked ? 'lucide:lock' : 'lucide:hard-drive'"
      class="size-10 text-muted-foreground"
    />
    <p class="max-w-md text-sm">
      {{
        locked
          ? t('files.drives.notice.locked', { name })
          : t('files.drives.notice.unmounted', { name })
      }}
    </p>
    <UiButton
      type="button"
      :loading="busy.has(drive.id)"
      data-testid="files-drive-notice-open"
      @click="onOpen"
    >
      {{
        locked
          ? t('files.drives.unlock.submit')
          : t('files.drives.notice.mount')
      }}
    </UiButton>
  </div>
</template>
