<script setup lang="ts">
/**
 * The icon of a header control (spec 090-app-header-bar, FR-023, research R9): a system app's
 * `lucide:` name is an Iconify icon; an extension's picture is looked up in the description's
 * `icons` table (filled by Rust from the extension's own bundle) and drawn as an `<img>`, or, with
 * `monochrome`, through its alpha channel in the primary colour. A path missing from the table draws
 * nothing. Sized 16 px; the glyphs are drawn in the primary colour (a raster picture keeps its own).
 */
const props = defineProps<{
  name?: string
  icons?: Record<string, string>
  monochrome?: boolean
}>()

const iconify = computed(() =>
  props.name?.startsWith('lucide:') ? props.name : undefined,
)
const url = computed(() =>
  props.name && !iconify.value ? props.icons?.[props.name] : undefined,
)

/** The picture as a mask, so `bg-primary` fills it with the primary colour. */
const mask = computed(() => ({
  maskImage: `url("${url.value}")`,
  maskRepeat: 'no-repeat',
  maskPosition: 'center',
  maskSize: 'contain',
}))
</script>

<template>
  <Icon
    v-if="iconify"
    :name="iconify"
    class="size-4 shrink-0 text-primary"
    :aria-hidden="true"
  />
  <span
    v-else-if="url && monochrome"
    class="size-4 shrink-0 bg-primary"
    aria-hidden="true"
    :style="mask"
  />
  <img
    v-else-if="url"
    :src="url"
    alt=""
    class="size-4 shrink-0 object-contain"
  />
</template>
