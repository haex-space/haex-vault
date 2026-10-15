<script setup lang="ts">
/**
 * "Allgemein → Lizenzen" (spec 053, FR-017, US5): every bundled icon set with its author, its
 * license (an SPDX identifier), its source and the full license text. The marks and logos of
 * companies, products and tools that some Material images show belong to their owners (the note
 * the pack itself carries).
 */
import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'
import type { IconSetLicense } from '@bindings/IconSetLicense'

const { t } = useI18n()
const { errString } = useErrorString()

const licenses = ref<IconSetLicense[]>([])
const error = ref<string | null>(null)

onMounted(async () => {
  try {
    licenses.value = await invoke<IconSetLicense[]>('file_icon_set_licenses')
  } catch (cause) {
    error.value = errString(cause)
  }
})

/** Only a web address is opened; anything else stays plain text. */
function isWeb(source: string): boolean {
  return /^https?:\/\//.test(source)
}
</script>

<template>
  <section class="flex flex-col gap-4" data-testid="settings-licenses">
    <p class="px-1 text-sm text-muted-foreground">
      {{ t('settings.licenses.intro') }}
    </p>

    <SettingsGroup
      v-for="item in licenses"
      :key="item.name"
      :label="item.name"
      :data-testid="`settings-license-${item.name}`"
    >
      <li class="flex flex-col gap-1 px-4 py-3 text-sm">
        <p>
          <span class="text-muted-foreground"
            >{{ t('settings.licenses.author') }}:</span
          >
          {{ item.author }}
        </p>
        <p>
          <span class="text-muted-foreground"
            >{{ t('settings.licenses.license') }}:</span
          >
          <span data-testid="settings-license-spdx">{{ item.license }}</span>
        </p>
        <p class="break-all">
          <span class="text-muted-foreground"
            >{{ t('settings.licenses.source') }}:</span
          >
          <a
            v-if="isWeb(item.source)"
            :href="item.source"
            class="underline"
            @click.prevent="openUrl(item.source)"
            >{{ item.source }}</a
          >
          <template v-else>{{ item.source }}</template>
        </p>
        <details class="mt-1">
          <summary class="cursor-pointer">
            {{ t('settings.licenses.text') }}
          </summary>
          <pre
            class="mt-2 max-h-80 overflow-auto whitespace-pre-wrap rounded-lg bg-background p-3 text-xs"
            >{{ item.text }}</pre>
        </details>
      </li>
    </SettingsGroup>

    <p v-if="error" class="px-1 text-sm text-destructive" role="alert">
      {{ error }}
    </p>
  </section>
</template>
