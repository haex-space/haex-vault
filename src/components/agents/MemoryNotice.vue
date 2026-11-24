<script setup lang="ts">
/**
 * The notice after an agent wrote to the shared memory (spec 097 FR-023, contracts "Events"): every
 * `agent-memory-written` of this device becomes a toast with the model, the page and the text, since
 * nothing else in the window shows what an agent stored. "Öffnen" opens "KI → Gedächtnis".
 */
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { toast } from 'vue-sonner'
import {
  EVENT_MEMORY_WRITTEN,
  memoryNoticeKey,
  type MemoryWritten,
} from '~/lib/agentMemory/pages'

const { t } = useI18n()
const openApp = useAction('wm.app.open')
let unlisten: UnlistenFn | undefined

function show(notice: MemoryWritten) {
  toast.info(
    t(memoryNoticeKey(notice.kind), {
      name: notice.authorName || t('settings.agentMemory.unknownAuthor'),
      title: notice.title,
      text: notice.text,
    }),
    {
      action: {
        label: t('settings.agentMemory.notice.open'),
        onClick: openMemory,
      },
    },
  )
}

function openMemory() {
  void openApp({ appId: 'system.settings', at: '/ai/memory' })
}

onMounted(async () => {
  try {
    unlisten = await listen<MemoryWritten>(EVENT_MEMORY_WRITTEN, (event) =>
      show(event.payload),
    )
  } catch (error) {
    console.error('[agent-memory] listening for notices failed', error)
  }
})
onBeforeUnmount(() => unlisten?.())
</script>

<template>
  <span hidden />
</template>
