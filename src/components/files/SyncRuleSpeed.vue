<script setup lang="ts">
/**
 * The speed of a running sync rule as the end of its progress line (" · 8.3 MB/s"); nothing while
 * it is not known yet. It lives in a component of its own because every row of the list needs its
 * own measurement. `bytes` is how much the rule has copied so far, or `null` while it is not
 * copying.
 */
import { formatSpeed } from '~/lib/transferSpeed'

const props = defineProps<{ bytes: number | null }>()

const speed = useTransferSpeed(() => props.bytes)
</script>

<template>
  <span v-if="speed !== null">{{ ` · ${formatSpeed(speed)}` }}</span>
</template>
