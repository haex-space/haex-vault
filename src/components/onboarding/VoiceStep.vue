<script setup lang="ts">
/**
 * The last onboarding step (spec 081 FR-012, operator 2026-11-06): the voice the assistant speaks
 * with. It offers the language's Pocket TTS model, the default speech model, so the assistant
 * speaks right after the onboarding without a download of its own. Asks before downloading (spec
 * 043 FR-028) like the steps before; skipping leaves the download to the settings.
 */
import { toast } from 'vue-sonner'
import type { SpeechModelOption } from '~/lib/assistant/speechModel'
import { soundPhraseKey } from '~/lib/assistant/speechModel'
import { humanBytes } from '~/lib/models/format'

const props = defineProps<{
  /** The model to offer; `null` while it loads or when there is none. */
  option: SpeechModelOption | null
  downloading: boolean
  /** Shown as a toast. */
  downloadError: string | null
}>()

const emit = defineEmits<{
  choose: []
  skip: []
  back: []
}>()

const { t } = useI18n()

watch(
  () => props.downloadError,
  (message) => {
    if (message)
      toast.error(`${t('onboarding.voice.downloadFailed')}: ${message}`)
  },
  { immediate: true },
)

const confirmOpen = ref(false)

function confirmed() {
  confirmOpen.value = false
  emit('choose')
}
</script>

<template>
  <div class="flex flex-col gap-4">
    <h2 class="text-xl font-semibold">
      {{ t('onboarding.voice.title') }}
    </h2>
    <p class="text-sm text-muted-foreground">
      {{ t('onboarding.voice.description') }}
    </p>

    <div v-if="!option" class="text-sm text-muted-foreground">
      {{ t('onboarding.voice.empty') }}
    </div>

    <button
      v-else
      type="button"
      class="relative flex flex-col gap-1 overflow-hidden rounded-md border border-input p-3 text-left hover:border-primary focus:outline-none focus:ring-2 focus:ring-ring disabled:cursor-progress disabled:opacity-50"
      :disabled="downloading"
      data-testid="onboarding-voice-download"
      @click="confirmOpen = true"
    >
      <ModelsDownloadBar :model-id="option.id" />
      <span class="relative z-10 text-base font-medium">{{ option.name }}</span>
      <span class="relative z-10 text-xs text-muted-foreground">
        {{ t(soundPhraseKey(option.engine)) }} ·
        {{ humanBytes(option.sizeBytes) }} · {{ option.license }}
      </span>
      <span class="relative z-10">
        <ModelsDownloadStatus :model-id="option.id" />
      </span>
    </button>

    <ModelsDownloadConfirmStep
      v-model:open="confirmOpen"
      :target="option && { kind: 'tts', id: option.id }"
      :name="option?.name ?? ''"
      @confirm="confirmed"
    />

    <div class="flex items-center justify-between">
      <UiButton
        type="button"
        variant="ghost"
        :disabled="downloading"
        @click="emit('back')"
      >
        {{ t('onboarding.wizard.back') }}
      </UiButton>
      <UiButton
        type="button"
        variant="outline"
        :disabled="downloading"
        data-testid="onboarding-voice-skip"
        @click="emit('skip')"
      >
        {{ t('onboarding.voice.skip') }}
      </UiButton>
    </div>
  </div>
</template>
