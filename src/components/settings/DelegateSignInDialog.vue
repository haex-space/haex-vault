<script setup lang="ts">
import { openUrl } from '@tauri-apps/plugin-opener'
import { toast } from 'vue-sonner'

/**
 * The sign-in of a subscription provider (spec 007): the link to open in the browser, the code to
 * enter there, and, where the provider hands the code back by hand, the field for it. The row
 * stays at two lines; this is what its sign-in needs beyond that.
 */
const props = defineProps<{
  title: string
  url: string | null
  /** The one-time code to enter on the page of the link. */
  deviceCode: string | null
  /** The provider waits for the code the page gives back. */
  awaitingCode: boolean
  /** The code is being exchanged. */
  submitting: boolean
}>()

const emit = defineEmits<{ submit: [] }>()
const open = defineModel<boolean>('open', { required: true })
const code = defineModel<string>('code', { required: true })

const { t } = useI18n()
const fieldLabels = useFieldLabels()

// The copy is confirmed by the icon, which turns into a check for a moment; there is no success line.
const copied = ref(false)
let copiedTimer: ReturnType<typeof setTimeout> | undefined

async function copyCodeAsync() {
  if (!props.deviceCode) return
  try {
    await navigator.clipboard.writeText(props.deviceCode)
    copied.value = true
    clearTimeout(copiedTimer)
    copiedTimer = setTimeout(() => (copied.value = false), 1500)
  } catch {
    toast.error(t('settings.cliDelegate.copyCodeFailed'))
  }
}

onBeforeUnmount(() => clearTimeout(copiedTimer))
</script>

<template>
  <UiDrawerModal v-model:open="open" :title="title">
    <template #content>
      <div class="flex flex-col gap-2" data-testid="cli-delegate-sign-in">
        <p class="text-sm text-muted-foreground">
          {{
            awaitingCode
              ? t('settings.cliDelegate.awaitingAuto')
              : t('settings.cliDelegate.connecting')
          }}
        </p>
        <template v-if="url">
          <p v-if="awaitingCode" class="text-sm">
            {{ t('settings.cliDelegate.openUrlPrompt') }}
          </p>
          <a
            :href="url"
            class="text-sm underline text-primary hover:text-primary/80 break-all"
            @click.prevent="openUrl(url)"
          >
            {{ url }}
          </a>
        </template>
        <p v-if="deviceCode" class="flex flex-wrap items-center gap-2 text-sm">
          <span>{{ t('settings.cliDelegate.enterCodePrompt') }}</span>
          <strong data-testid="cli-delegate-device-code">{{
            deviceCode
          }}</strong>
          <UiButton
            type="button"
            variant="ghost"
            size="icon-sm"
            :aria-label="
              copied
                ? t('settings.cliDelegate.codeCopied')
                : t('settings.cliDelegate.copyCode')
            "
            :tooltip="
              copied
                ? t('settings.cliDelegate.codeCopied')
                : t('settings.cliDelegate.copyCode')
            "
            data-testid="cli-delegate-copy-code"
            @click="copyCodeAsync"
          >
            <Icon
              :name="copied ? 'lucide:check' : 'lucide:copy'"
              class="size-4"
              aria-hidden="true"
            />
          </UiButton>
        </p>
        <template v-if="awaitingCode">
          <UiInput
            v-model="code"
            :label="t('settings.cliDelegate.codeInputLabel')"
            :labels="fieldLabels.input.value"
            label-bg="var(--background)"
            type="text"
            autocomplete="off"
            @keyup.enter="emit('submit')"
          />
          <div class="flex items-center gap-3 flex-wrap">
            <UiButton
              type="button"
              :loading="submitting"
              :disabled="!code.trim() || submitting"
              @click="emit('submit')"
            >
              {{ t('settings.cliDelegate.submitCode') }}
            </UiButton>
          </div>
        </template>
      </div>
    </template>
  </UiDrawerModal>
</template>
