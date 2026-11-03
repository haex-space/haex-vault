<script setup lang="ts">
/**
 * "Allgemein → Erscheinungsbild" (spec 042, FR-016): the workspace background. The user picks the
 * image with the system's file chooser (a native file input, no file access for holzi beyond the
 * chosen file); it is scaled down and stored for the whole vault. Removing goes through
 * `settings.appearance.removeBackground`, which an agent can call too.
 */
import { toast } from 'vue-sonner'

const { t } = useI18n()
const { errString } = useErrorString()
const { background, setFromFileAsync } = useWorkspaceBackground()
const removeBackground = useActionOrThrow(
  'settings.appearance.removeBackground',
)

const input = useTemplateRef<HTMLInputElement>('input')
const busy = ref(false)

async function runAsync(work: () => Promise<unknown>) {
  busy.value = true
  try {
    await work()
  } catch (e) {
    toast.error(`${t('settings.background.failed')}: ${errString(e)}`)
  } finally {
    busy.value = false
  }
}

function choose() {
  input.value?.click()
}

async function onPickedAsync(event: Event) {
  const target = event.target as HTMLInputElement
  const file = target.files?.[0]
  // The same file can be chosen again after a failure.
  target.value = ''
  if (file) await runAsync(() => setFromFileAsync(file))
}

async function removeAsync() {
  await runAsync(() => removeBackground())
}
</script>

<template>
  <section class="flex flex-col gap-2">
    <SettingsGroup>
      <SettingsRow
        :title="t('settings.background.label')"
        :description="t('settings.background.description')"
      >
        <div class="flex flex-wrap justify-end gap-2">
          <input
            ref="input"
            type="file"
            accept="image/*"
            class="hidden"
            data-testid="settings-background-input"
            @change="onPickedAsync"
          />
          <UiButton
            variant="outline"
            :loading="busy"
            data-testid="settings-background-choose"
            @click="choose"
          >
            {{ t('settings.background.choose') }}
          </UiButton>
          <UiButton
            v-if="background"
            variant="outline"
            :disabled="busy"
            data-testid="settings-background-remove"
            @click="removeAsync"
          >
            {{ t('settings.background.remove') }}
          </UiButton>
        </div>
      </SettingsRow>
    </SettingsGroup>
  </section>
</template>
