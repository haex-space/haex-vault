<script setup lang="ts">
/**
 * The icon of a folder known only by its path, as the list shows it (spec 053): the sidebar's added
 * folders (spec 071) look like their entry in the folder, "Bilder" with the set's picture folder.
 */
import { breadcrumbs } from '~/lib/files/state'

const props = defineProps<{ path: string }>()

const name = computed(() => breadcrumbs(props.path).at(-1)?.name ?? '')
const icon = useFileIcon(() => ({
  name: name.value,
  isDir: true,
  noAccess: false,
  encrypted: null,
}))
</script>

<template>
  <img
    v-if="icon?.kind === 'image'"
    :src="icon.url"
    alt=""
    class="size-full object-contain"
    draggable="false"
  />
  <Icon v-else :name="icon?.name ?? 'lucide:folder'" class="size-full" />
</template>
