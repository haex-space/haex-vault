<script setup lang="ts">
/**
 * The picture of an entry (spec 044 FR-003, FR-005): a thumbnail for images in the grid, else an
 * icon by kind. Encrypted folders and entries that cannot be read show their state (spec 048). Thumbnails are asked for only while the entry is on screen (the virtual list
 * mounts visible rows only).
 */
import type { Entry } from '@bindings/Entry'
import type { SourceRef } from '@bindings/SourceRef'
import { encryptedLook } from '~/lib/files/encrypted'
import { viewerKind } from '~/lib/files/viewerKind'

const props = defineProps<{
  source: SourceRef
  entry: Entry
  thumbnail?: boolean
}>()

const { t } = useI18n()
const { thumbnailFor } = useFilesThumbnails()
const look = computed(() => encryptedLook(props.entry))
const url = ref<string | null>(null)

const icon = computed(() => {
  if (look.value) return look.value.icon
  if (props.entry.kind === 'dir') {
    return props.entry.noAccess ? 'lucide:folder-lock' : 'lucide:folder'
  }
  switch (viewerKind(props.entry.name)) {
    case 'image':
      return 'lucide:image'
    case 'video':
      return 'lucide:film'
    case 'audio':
      return 'lucide:music'
    case 'pdf':
    case 'text':
      return 'lucide:file-text'
    default:
      return 'lucide:file'
  }
})

watch(
  () => [
    props.entry.path,
    props.entry.size,
    props.entry.modifiedMs,
    props.thumbnail,
  ],
  async () => {
    url.value = null
    if (!props.thumbnail || props.entry.kind !== 'file') return
    if (look.value) return
    if (viewerKind(props.entry.name) !== 'image') return
    const asked = props.entry.path
    const result = await thumbnailFor(props.source, props.entry).catch(
      () => null,
    )
    if (asked === props.entry.path && result?.kind === 'ready')
      url.value = result.url
  },
  { immediate: true },
)
</script>

<template>
  <img
    v-if="url"
    :src="url"
    alt=""
    class="size-full object-contain"
    draggable="false"
  />
  <Icon
    v-else
    :name="icon"
    class="size-full text-muted-foreground"
    :title="look?.stateKey ? t(look.stateKey) : undefined"
    :data-encrypted="entry.encrypted?.state"
  />
</template>
