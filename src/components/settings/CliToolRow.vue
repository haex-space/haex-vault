<script setup lang="ts">
import {
  formatBytes,
  installable,
  installing,
  isolationKey,
  phaseKey,
  progressFraction,
  revertible,
  stateKey,
  updatable,
  usable,
} from '~/lib/cli-tools'
import type { CliToolProgress } from '~/types/bindings/CliToolProgress'
import type { CliToolStatus } from '~/types/bindings/CliToolStatus'

/**
 * One tool of a vault (spec 094): its state, the install with the size it takes, the progress of a
 * running install and its cancel. The new state is the feedback of an install; there is no
 * success line. Nothing here updates silently: every change is a click.
 */
const props = defineProps<{
  status: CliToolStatus
  progress?: CliToolProgress
}>()

const emit = defineEmits<{
  install: []
  revert: []
  cancel: []
}>()

const { t } = useI18n()

const percent = computed(() => {
  const fraction = progressFraction(props.progress)
  return fraction === null ? null : fraction * 100
})

const description = computed(() => {
  const status = props.status
  if (installing(status)) return t(phaseKey(props.progress))
  if (usable(status) && status.installedVersion) {
    return t('settings.cliTools.state.installedVersion', {
      version: status.installedVersion,
    })
  }
  return t(stateKey(status))
})

/** What else the row says about the version in use, each as a line of its own. */
const notes = computed(() => {
  const status = props.status
  if (!usable(status)) return []
  const lines: string[] = []
  if (status.previousVersion) {
    lines.push(
      t('settings.cliTools.unconfirmed', { previous: status.previousVersion }),
    )
  } else if (
    status.state === 'update_available' &&
    status.latestListedVersion
  ) {
    lines.push(
      t('settings.cliTools.updateAvailable', {
        version: status.latestListedVersion,
      }),
    )
  }
  if (status.outdated) lines.push(t('settings.cliTools.outdated'))
  if (status.untested) lines.push(t('settings.cliTools.untested'))
  return lines
})

/** The size is shown next to the install action, so the click is an informed one. */
const sizeNote = computed(() => {
  const { downloadBytes, unpackedBytes } = props.status
  if (!installable(props.status) || downloadBytes === null) return null
  return t('settings.cliTools.size', {
    download: formatBytes(downloadBytes),
    disk: formatBytes(unpackedBytes ?? downloadBytes),
  })
})
</script>

<template>
  <SettingsRow
    :title="t('settings.cliTools.title', { name: status.displayName })"
    :data-testid="`settings-row-cli-tool-${status.vendor}`"
  >
    <template v-if="installing(status)" #backdrop>
      <UiProgress :value="percent" variant="backdrop" />
    </template>
    <template #description>
      <span>{{ description }}</span>
      <span v-if="sizeNote" class="block">{{ sizeNote }}</span>
      <span
        v-for="(note, index) in notes"
        :key="index"
        class="block"
        :data-testid="`cli-tool-note-${status.vendor}`"
      >
        {{ note }}
      </span>
      <span
        v-if="usable(status)"
        class="block"
        :data-testid="`cli-tool-isolation-${status.vendor}`"
      >
        {{ t(isolationKey(status)) }}
      </span>
    </template>
    <UiButton
      v-if="installable(status)"
      type="button"
      variant="outline"
      :data-testid="`cli-tool-install-${status.vendor}`"
      @click="emit('install')"
    >
      {{ t('settings.cliTools.install') }}
    </UiButton>
    <UiButton
      v-else-if="updatable(status)"
      type="button"
      variant="outline"
      :data-testid="`cli-tool-update-${status.vendor}`"
      @click="emit('install')"
    >
      {{ t('settings.cliTools.update') }}
    </UiButton>
    <UiButton
      v-else-if="revertible(status)"
      type="button"
      variant="outline"
      :data-testid="`cli-tool-revert-${status.vendor}`"
      @click="emit('revert')"
    >
      {{ t('settings.cliTools.revert', { version: status.previousVersion }) }}
    </UiButton>
    <UiButton
      v-else-if="installing(status)"
      type="button"
      variant="outline"
      :data-testid="`cli-tool-cancel-${status.vendor}`"
      @click="emit('cancel')"
    >
      {{ t('settings.cliTools.cancel') }}
    </UiButton>
  </SettingsRow>
</template>
