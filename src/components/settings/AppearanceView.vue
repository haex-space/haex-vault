<script setup lang="ts">
/**
 * "Allgemein → Erscheinungsbild" (spec 023-settings-app FR-013, spec 035-appearance-and-fields,
 * spec 042): the colour scheme, then the accent colour, the backgrounds, the text and component tints like the COSMIC dialog "Aussehen", with
 * import, export and reset. Every choice is saved as soon as it is made (no save button).
 */
const { t } = useI18n()

const status = ref<{ kind: 'saved' | 'failed'; text: string } | null>(null)
const saved = () =>
  (status.value = { kind: 'saved', text: t('settings.appearance.saved') })
const done = (text: string) => (status.value = { kind: 'saved', text })
const failed = (text: string) => (status.value = { kind: 'failed', text })
</script>

<template>
  <div class="flex flex-col gap-4">
    <SettingsColorSchemeSetting />
    <SettingsBackgroundSetting />
    <SettingsFileIconSetSetting />

    <div class="flex flex-wrap items-center justify-end gap-2">
      <SettingsAppearanceFileButtons @done="done" @failed="failed" />
    </div>

    <SettingsGroup>
      <SettingsAppearanceColorRow
        control="accent"
        title-key="accent"
        @saved="saved"
        @failed="failed"
      />
      <SettingsAppearanceColorRow
        control="window"
        title-key="window"
        @saved="saved"
        @failed="failed"
      />
      <SettingsAppearanceColorRow
        control="container"
        title-key="container"
        hint-key="containerHint"
        @saved="saved"
        @failed="failed"
      />
      <SettingsAppearanceColorRow
        control="text"
        title-key="text"
        hint-key="textHint"
        @saved="saved"
        @failed="failed"
      />
      <SettingsAppearanceColorRow
        control="component"
        title-key="component"
        hint-key="componentHint"
        @saved="saved"
        @failed="failed"
      />
      <SettingsAppearanceWindowHint @saved="saved" @failed="failed" />
    </SettingsGroup>

    <div class="flex flex-wrap items-center gap-2">
      <SettingsAppearanceResetButton
        @done="done(t('settings.appearance.saved'))"
        @failed="failed"
      />
    </div>

    <p
      v-if="status?.kind === 'saved'"
      class="px-1 text-xs text-success"
      role="status"
    >
      {{ status.text }}
    </p>
    <p
      v-if="status?.kind === 'failed'"
      class="px-1 text-xs text-destructive"
      role="alert"
      data-testid="appearance-error"
    >
      {{ status.text }}
    </p>
  </div>
</template>
