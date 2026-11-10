<script setup lang="ts">
/**
 * "Allgemein → Erscheinungsbild" (spec 023-settings-app FR-013, spec 035-appearance-and-fields,
 * spec 042): the colour scheme, then the accent colour, the backgrounds, the text and component tints like the COSMIC dialog "Aussehen", with
 * import, export and reset. Every choice is saved as soon as it is made (no save button) and
 * silently; a failure shows as a toast, and so does the result of an import or an export.
 */
import { toast } from 'vue-sonner'

/** The finished import or export, which writes or reads a file the user chose. */
const done = (text: string) => toast.success(text)
const failed = (text: string) => toast.error(text)
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
        @failed="failed"
      />
      <SettingsAppearanceColorRow
        control="window"
        title-key="window"
        @failed="failed"
      />
      <SettingsAppearanceColorRow
        control="container"
        title-key="container"
        hint-key="containerHint"
        @failed="failed"
      />
      <SettingsAppearanceColorRow
        control="text"
        title-key="text"
        hint-key="textHint"
        @failed="failed"
      />
      <SettingsAppearanceColorRow
        control="component"
        title-key="component"
        hint-key="componentHint"
        @failed="failed"
      />
      <SettingsAppearanceWindowHint @failed="failed" />
    </SettingsGroup>

    <div class="flex flex-wrap items-center gap-2">
      <SettingsAppearanceResetButton @failed="failed" />
    </div>
  </div>
</template>
