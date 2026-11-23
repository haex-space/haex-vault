<script setup lang="ts">
/**
 * The first step of signing in to a provider that uses the account's own credentials (spec 096): the
 * e-mail address of the account. No password is typed here; HaexVault then shows a link and a code
 * to confirm in the browser.
 */
defineProps<{
  title: string
  /** The sign-in has been started: the button waits for the link. */
  submitting: boolean
}>()

const emit = defineEmits<{ submit: [] }>()
const open = defineModel<boolean>('open', { required: true })
const email = defineModel<string>('email', { required: true })

const { t } = useI18n()
const fieldLabels = useFieldLabels()

/** A plain check that the text can be an address; the service decides whether the account exists. */
const looksLikeAddress = computed(() =>
  /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.value.trim()),
)

function submit() {
  if (looksLikeAddress.value) emit('submit')
}
</script>

<template>
  <UiDrawerModal v-model:open="open" :title="title">
    <template #content>
      <div class="flex flex-col gap-3" data-testid="cli-delegate-email">
        <p class="text-sm text-muted-foreground">
          {{ t('settings.cliDelegate.emailHint') }}
        </p>
        <UiInput
          v-model="email"
          :label="t('settings.cliDelegate.emailLabel')"
          :labels="fieldLabels.input.value"
          label-bg="var(--background)"
          type="email"
          autocomplete="email"
          data-testid="cli-delegate-email-input"
          @keyup.enter="submit"
        />
        <div class="flex items-center gap-3 flex-wrap">
          <UiButton
            type="button"
            :loading="submitting"
            :disabled="!looksLikeAddress || submitting"
            data-testid="cli-delegate-email-continue"
            @click="submit"
          >
            {{ t('settings.cliDelegate.emailContinue') }}
          </UiButton>
        </div>
      </div>
    </template>
  </UiDrawerModal>
</template>
