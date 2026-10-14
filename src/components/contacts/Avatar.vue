<script setup lang="ts">
/**
 * The photo of a contact, or its initials when it has none (spec 051, FR-003). The JPEG is loaded
 * once per hash and released when the component goes.
 */
const props = defineProps<{
  name: string
  photo: string | null
  size?: 'sm' | 'lg'
}>()

const { photoAsync } = useContacts()
const url = ref<string | null>(null)

const initials = computed(() => {
  const words = props.name.trim().split(/\s+/).filter(Boolean)
  const letters = words.length > 1 ? [words[0], words.at(-1)] : words
  return letters
    .map((word) => [...(word ?? '')][0] ?? '')
    .join('')
    .toUpperCase()
})

watch(
  () => props.photo,
  async (hash) => {
    if (url.value) URL.revokeObjectURL(url.value)
    url.value = null
    if (!hash) return
    try {
      const bytes = await photoAsync(hash)
      if (props.photo === hash) {
        url.value = URL.createObjectURL(
          new Blob([bytes], { type: 'image/jpeg' }),
        )
      }
    } catch {
      // No photo is shown; the initials stand in.
    }
  },
  { immediate: true },
)

onBeforeUnmount(() => {
  if (url.value) URL.revokeObjectURL(url.value)
})
</script>

<template>
  <span
    class="flex shrink-0 items-center justify-center overflow-hidden rounded-full bg-primary/15 font-medium text-primary"
    :class="size === 'lg' ? 'size-20 text-2xl' : 'size-9 text-sm'"
    aria-hidden="true"
  >
    <img v-if="url" :src="url" alt="" class="size-full object-cover" />
    <span v-else-if="initials">{{ initials }}</span>
    <Icon
      v-else
      name="lucide:user"
      :class="size === 'lg' ? 'size-8' : 'size-4'"
    />
  </span>
</template>
