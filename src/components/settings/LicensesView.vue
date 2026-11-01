<script setup lang="ts">
/**
 * "Allgemein → Lizenzen" (spec 053, FR-017, US5): every bundled icon set with its author, its
 * license (an SPDX identifier), its source and the full license text. The marks and logos of
 * companies, products and tools that some Material images show belong to their owners (the note
 * the pack itself carries). Below them the built-in voices and the speech models the assistant
 * loads, each with the credit its licence asks for (spec 066 T032).
 */
import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'
import type { IconSetLicense } from '@bindings/IconSetLicense'
import type { SpeechCredit } from '~/composables/useTtsVoices'

const { t, locale } = useI18n()
const { errString } = useErrorString()

const { creditsAsync } = useTtsVoices()

const licenses = ref<IconSetLicense[]>([])
const credits = ref<SpeechCredit[]>([])
const error = ref<string | null>(null)

onMounted(async () => {
  const [licensesResult, creditsResult] = await Promise.allSettled([
    invoke<IconSetLicense[]>('file_icon_set_licenses'),
    creditsAsync(),
  ])
  if (licensesResult.status === 'fulfilled')
    licenses.value = licensesResult.value
  else error.value = errString(licensesResult.reason)
  if (creditsResult.status === 'fulfilled') credits.value = creditsResult.value
  else error.value ??= errString(creditsResult.reason)
})

const CREDIT_KINDS = ['voice', 'speechModel', 'recognitionModel'] as const

function creditsOf(kind: SpeechCredit['kind']): SpeechCredit[] {
  return credits.value.filter((credit) => credit.kind === kind)
}

/** A voice or speech model with its language ("Lena · Deutsch"). */
function creditTitle(credit: SpeechCredit): string {
  if (credit.kind !== 'voice' || !credit.language) return credit.name
  const names = new Intl.DisplayNames([locale.value], { type: 'language' })
  return `${credit.name} · ${names.of(credit.language) ?? credit.language}`
}

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

    <template v-if="credits.length > 0">
      <p class="mt-4 px-1 text-sm text-muted-foreground">
        {{ t('settings.licenses.speechIntro') }}
      </p>
      <SettingsGroup
        v-for="kind in CREDIT_KINDS"
        :key="kind"
        :label="t(`settings.licenses.speech.${kind}`)"
        :data-testid="`settings-license-speech-${kind}`"
      >
        <li
          v-for="credit in creditsOf(kind)"
          :key="credit.name"
          class="flex flex-col gap-1 px-4 py-3 text-sm"
        >
          <p class="font-medium">{{ creditTitle(credit) }}</p>
          <p>{{ credit.attribution }}</p>
          <p>
            <span class="text-muted-foreground"
              >{{ t('settings.licenses.license') }}:</span
            >
            {{ credit.license }}
          </p>
          <p class="break-all">
            <span class="text-muted-foreground"
              >{{ t('settings.licenses.source') }}:</span
            >
            <a
              :href="credit.source"
              class="underline"
              @click.prevent="openUrl(credit.source)"
              >{{ credit.source }}</a
            >
          </p>
        </li>
      </SettingsGroup>
    </template>

    <p v-if="error" class="px-1 text-sm text-destructive" role="alert">
      {{ error }}
    </p>
  </section>
</template>
