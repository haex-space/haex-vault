<template>
  <div class="p-4 space-y-4 max-w-2xl mx-auto">
    <div
      v-if="isExpired && (isEditing || form.expiresAt)"
      class="flex items-center gap-2 px-3 py-2 bg-warning/10 border border-warning/30 rounded-md text-sm"
    >
      <UIcon
        name="i-lucide-alert-triangle"
        class="size-4 text-warning shrink-0"
      />
      <span>{{ t('expired') }}</span>
    </div>

    <div v-if="isEditing || form.title">
      <UiInput
        v-model="form.title"
        v-model:errors="errors.title"
        :label="t('fields.title')"
        :placeholder="t('fields.titlePlaceholder')"
        :read-only="!isEditing"
        :required="isEditing"
        :with-copy-button="!isEditing"
      />
    </div>

    <div v-if="isEditing || form.username">
      <UiInput
        v-model="form.username"
        :label="t('fields.username')"
        leading-icon="i-lucide-user"
        :read-only="!isEditing"
        with-copy-button
      />
    </div>

    <div v-if="isEditing || form.password">
      <div class="flex items-start gap-2">
        <UiInputPassword
          v-model="form.password"
          :label="t('fields.password')"
          :read-only="!isEditing"
          with-copy-button
          class="flex-1"
        />
        <UiButton
          v-if="isEditing"
          :tooltip="t('fields.generate')"
          icon="i-lucide-wand-sparkles"
          color="neutral"
          variant="outline"
          type="button"
          class="shrink-0"
          @click="emit('openGenerator')"
        />
      </div>
    </div>

    <div v-if="isEditing || form.url">
      <UiInput
        v-model="form.url"
        :label="t('fields.url')"
        leading-icon="i-lucide-globe"
        placeholder="https://…"
        :read-only="!isEditing"
        with-copy-button
      />
    </div>

    <div v-if="isEditing || form.tagNames.length">
      <HaexSystemPasswordsEditorTagPicker
        v-if="isEditing"
        v-model="form.tagNames"
        :label="t('fields.tags')"
      />
      <div v-else>
        <p class="text-xs font-medium text-muted mb-1">
          {{ t('fields.tags') }}
        </p>
        <div class="flex flex-wrap gap-1">
          <UBadge
            v-for="name in form.tagNames"
            :key="name"
            :label="name"
            color="neutral"
            variant="soft"
          />
        </div>
      </div>
      <p
        v-if="errors.tags.length"
        class="mt-1 text-xs text-error"
      >
        {{ errors.tags[0] }}
      </p>
    </div>

    <div v-if="isEditing || form.note">
      <UiTextarea
        v-model="form.note"
        :label="t('fields.note')"
        :rows="3"
        :read-only="!isEditing"
      />
    </div>

    <div v-if="isEditing || form.expiresAt">
      <UiInput
        v-model="form.expiresAt"
        :label="t('fields.expiresAt')"
        type="date"
        leading-icon="i-lucide-calendar"
        :read-only="!isEditing"
      />
    </div>

    <div
      v-if="isEditing"
      class="flex items-end gap-3"
    >
      <div class="flex-1 min-w-0">
        <label class="text-xs font-medium text-highlighted mb-1 block">
          {{ t('fields.icon') }}
        </label>
        <HaexSystemPasswordsEditorIconPicker
          v-model="form.icon"
          :color="form.color || undefined"
        />
      </div>
      <div class="flex flex-col gap-1">
        <label class="text-xs font-medium text-highlighted">
          {{ t('fields.color') }}
        </label>
        <input
          v-model="form.color"
          type="color"
          class="size-10 rounded-md border border-default cursor-pointer p-0 bg-transparent"
        >
      </div>
    </div>

    <!-- OTP -->
    <div
      v-if="isEditing || otpCode"
      class="border border-default rounded-md p-3 space-y-3"
    >
      <div class="flex items-center gap-2">
        <UIcon
          name="i-lucide-shield-check"
          class="size-4 text-primary"
        />
        <p class="text-sm font-medium">
          {{ t('fields.otp') }}
        </p>
      </div>

      <template v-if="isEditing">
        <UiInput
          v-model="form.otpSecret"
          :label="t('fields.otpSecret')"
          placeholder="JBSWY3DPEHPK3PXP"
        />
        <div class="grid grid-cols-3 gap-2">
          <UiInput
            v-model.number="form.otpDigits"
            :label="t('fields.otpDigits')"
            type="number"
            min="6"
            max="10"
          />
          <UiInput
            v-model.number="form.otpPeriod"
            :label="t('fields.otpPeriod')"
            type="number"
            min="10"
            max="120"
          />
          <USelect
            v-model="form.otpAlgorithm"
            :items="[...otpAlgorithms]"
            size="md"
          />
        </div>
      </template>

      <div
        v-else
        class="flex items-center gap-3 px-3 py-2 rounded-md bg-elevated/30"
      >
        <span
          class="flex-1 font-mono text-xl tracking-[0.3em] select-all"
        >{{ otpFormatted }}</span>
        <div class="relative size-8 shrink-0">
          <svg
            viewBox="0 0 36 36"
            class="size-8 -rotate-90"
          >
            <circle
              cx="18"
              cy="18"
              r="15.5"
              fill="none"
              stroke-width="2.5"
              class="stroke-default"
            />
            <circle
              cx="18"
              cy="18"
              r="15.5"
              fill="none"
              stroke-width="2.5"
              stroke-linecap="round"
              :stroke-dasharray="otpDashArray"
              class="stroke-primary transition-[stroke-dasharray] duration-1000 ease-linear"
            />
          </svg>
          <span
            class="absolute inset-0 flex items-center justify-center text-[10px] tabular-nums"
          >{{ otpRemaining }}</span>
        </div>
        <UiButton
          :tooltip="copiedOtp ? t('copied') : t('copy')"
          :icon="copiedOtp ? 'i-lucide-check' : 'i-lucide-copy'"
          :color="copiedOtp ? 'success' : 'neutral'"
          variant="ghost"
          type="button"
          class="shrink-0"
          @click="() => copyOtp(otpCode ?? '')"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { EditorForm } from '~/composables/passwords/usePasswordEditorForm'

const { t } = useI18n()

defineProps<{
  form: EditorForm
  errors: { title: string[]; tags: string[] }
  isEditing: boolean
  isExpired: boolean
  otpCode: string | null
  otpFormatted: string
  otpRemaining: number
  otpDashArray: string
  copiedOtp: boolean
  copyOtp: (value: string) => void
  otpAlgorithms: readonly string[]
}>()

const emit = defineEmits<{
  openGenerator: []
}>()
</script>

<style scoped>
/* Strip browser chrome so the color input renders as a flat swatch. */
input[type='color']::-webkit-color-swatch-wrapper {
  padding: 0;
}
input[type='color']::-webkit-color-swatch {
  border: none;
  border-radius: 5px;
}
input[type='color']::-moz-color-swatch {
  border: none;
  border-radius: 5px;
}
</style>

<i18n lang="yaml">
de:
  copy: Kopieren
  copied: Kopiert
  expired: Dieser Eintrag ist abgelaufen.
  fields:
    title: Titel
    titlePlaceholder: z.B. GitHub
    tags: Tags
    username: Nutzername
    password: Passwort
    generate: Generieren
    url: URL
    note: Notiz
    expiresAt: Ablaufdatum
    icon: Icon
    color: Farbe
    otp: Einmalcode (TOTP)
    otpSecret: Base32 Secret
    otpDigits: Stellen
    otpPeriod: Periode (s)
en:
  copy: Copy
  copied: Copied
  expired: This entry has expired.
  fields:
    title: Title
    titlePlaceholder: e.g. GitHub
    tags: Tags
    username: Username
    password: Password
    generate: Generate
    url: URL
    note: Note
    expiresAt: Expires at
    icon: Icon
    color: Color
    otp: One-time code (TOTP)
    otpSecret: Base32 secret
    otpDigits: Digits
    otpPeriod: Period (s)
</i18n>
