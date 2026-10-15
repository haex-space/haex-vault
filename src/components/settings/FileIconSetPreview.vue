<script setup lang="ts">
/**
 * The preview of an icon set (spec 053, FR-012): a sample folder with the typical entries, drawn
 * through `useFileIcon` for the given set, so a set shows exactly what the file browser will show.
 */
import type { IconQuery } from '@bindings/IconQuery'
import type { IconSetRef } from '@bindings/IconSetRef'

defineProps<{ set: IconSetRef }>()

const entry = (name: string, isDir = false): IconQuery => ({
  name,
  isDir,
  noAccess: false,
  encrypted: null,
})

/** A folder, an image, a video, music, a PDF, text, code and an archive. */
const SAMPLE: readonly IconQuery[] = [
  entry('Dokumente', true),
  entry('foto.png'),
  entry('film.mp4'),
  entry('lied.mp3'),
  entry('bericht.pdf'),
  entry('notizen.txt'),
  entry('main.rs'),
  entry('archiv.zip'),
]
</script>

<template>
  <span class="flex items-center gap-1.5" aria-hidden="true">
    <SettingsFileIconSample
      v-for="query in SAMPLE"
      :key="query.name"
      :set="set"
      :query="query"
    />
  </span>
</template>
