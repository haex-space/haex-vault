<script setup lang="ts">
/**
 * The icon of a folder known only by its path, as the list shows it (spec 053): every entry of "Orte"
 * (spec 071) looks like its folder in the list, "Bilder" with the set's picture folder, and follows
 * a change of the icon set as the list does. `name` asks for another name than the folder's (the home
 * folder is named after the user, its place asks for "home").
 */
import { breadcrumbs } from '~/lib/files/state'

const props = defineProps<{ path: string; name?: string }>()

const name = computed(
  () => props.name ?? breadcrumbs(props.path).at(-1)?.name ?? '',
)
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
  <Icon
    v-else
    :name="icon?.name ?? 'lucide:folder'"
    class="size-full text-muted-foreground"
  />
</template>
