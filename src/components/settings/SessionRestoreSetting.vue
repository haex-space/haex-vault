<script setup lang="ts">
/**
 * The setting "Sitzung wiederherstellen" (spec 022-session-restore, FR-004, contracts/wm-session.md
 * §5): one switch for the whole vault since spec 023 (FR-024), saved as soon as it flips, without
 * a save button. Turning it off deletes the saved session. Writes go through the catalog action
 * (spec 020 FR-024).
 */
import { toast } from 'vue-sonner'
import type { RestoreState } from '~/lib/wm/sessionSync'

const { t } = useI18n()
const { errString } = useErrorString()
const wm = useWindowManagerStore()
const setRestore = useActionOrThrow('settings.sessionRestore.set')

const restore = ref<RestoreState | null>(null)
const busy = ref(false)

async function reloadAsync() {
  try {
    restore.value = await wm.getSessionRestore()
  } catch (error: unknown) {
    toast.error(`${t('errors.prefLoadFailed')}: ${errString(error)}`)
  }
}

async function toggleAsync(enabled: boolean) {
  if (!restore.value || busy.value) return
  busy.value = true
  try {
    restore.value = (await setRestore({ enabled })) as RestoreState
  } catch (error: unknown) {
    toast.error(`${t('settings.sessionRestore.failed')}: ${errString(error)}`)
    await reloadAsync()
  } finally {
    busy.value = false
  }
}

onMounted(reloadAsync)
onVaultTablesChanged(['preferences'], () => {
  if (!busy.value) return reloadAsync()
})
</script>

<template>
  <section class="flex flex-col gap-2">
    <SettingsGroup v-if="restore">
      <SettingsRow
        :title="t('settings.sessionRestore.title')"
        :description="t('settings.sessionRestore.description')"
        label-for="session-restore-switch"
      >
        <ShadcnSwitch
          id="session-restore-switch"
          :model-value="restore.enabled"
          :disabled="busy"
          data-testid="session-restore-switch"
          @update:model-value="toggleAsync($event === true)"
        />
      </SettingsRow>
    </SettingsGroup>
  </section>
</template>
