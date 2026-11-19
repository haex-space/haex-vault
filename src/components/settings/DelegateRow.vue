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
 * One subscription provider of a vault (Claude Code, Codex): the program and its sign-in in one
 * row (spec 094). The row always has two lines, the name and one line of state, so it keeps its
 * height through every state; what else there is to know sits behind the info button. Without
 * the program the row offers the install and nothing else; with it, the sign-in, the model
 * refresh, an update or the way back, and the removal of program and connection. The new state is
 * the feedback of every click; there is no success line. Nothing here updates silently.
 */
const props = defineProps<{
  vendor: string
  title: string
  status?: CliToolStatus
  progress?: CliToolProgress
  /** A provider of this vendor is stored in the vault. */
  connected: boolean
  /** A sign-in of this vendor is running. */
  connecting: boolean
  refreshing: boolean
  removing: boolean
  /** The device cannot run delegates, so there is nothing to sign in to. */
  connectBlocked: boolean
}>()

const emit = defineEmits<{
  install: []
  revert: []
  cancel: []
  connect: []
  showSignIn: []
  refresh: []
  remove: []
}>()

const { t } = useI18n()
const status = computed(() => props.status)

const infoOpen = ref(false)
const removeOpen = ref(false)

const percent = computed(() => {
  const fraction = progressFraction(props.progress)
  return fraction === null ? null : fraction * 100
})

/** The size is shown before the install, so the click is an informed one. */
const sizeNote = computed(() => {
  const status = props.status
  if (!status) return null
  const { downloadBytes, unpackedBytes } = status
  if (!installable(status) || downloadBytes === null) return null
  return t('settings.cliTools.size', {
    download: formatBytes(downloadBytes),
    disk: formatBytes(unpackedBytes ?? downloadBytes),
  })
})

/** The first part of the state line. */
const lead = computed(() => {
  const status = props.status
  if (!status) return t('settings.cliTools.statusUnavailable')
  if (installing(status)) return t(phaseKey(props.progress))
  if (usable(status)) {
    return status.installedVersion
      ? t('settings.cliTools.line.version', {
          version: status.installedVersion,
        })
      : t('settings.cliTools.state.installed')
  }
  if (status.state === 'not_installed') {
    return status.downloadBytes === null
      ? t('settings.cliTools.line.notInstalled')
      : t('settings.cliTools.line.notInstalledSize', {
          download: formatBytes(status.downloadBytes),
        })
  }
  return t(stateKey(status))
})

/** The second part of the state line: whether the program is signed in. */
const tail = computed(() => {
  if (!props.status)
    return props.connected
      ? { text: t('settings.cliDelegate.connected'), success: true }
      : null
  if (!usable(props.status) || installing(props.status)) return null
  if (props.connecting) {
    return { text: t('settings.cliDelegate.connecting'), success: false }
  }
  return props.connected
    ? { text: t('settings.cliDelegate.connected'), success: true }
    : { text: t('settings.cliDelegate.notConnected'), success: false }
})

const line = computed(() =>
  tail.value ? `${lead.value} · ${tail.value.text}` : lead.value,
)

/** What else there is to know about the version in use, each as a line of its own. */
const notes = computed(() => {
  const status = props.status
  if (!status || !usable(status)) return []
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

/** The state in words for the info: the line without the sign-in. */
const stateText = computed(() => {
  const status = props.status
  if (!status) return ''
  if (installing(status)) return t(phaseKey(props.progress))
  if (usable(status) && status.installedVersion) {
    return t('settings.cliTools.state.installedVersion', {
      version: status.installedVersion,
    })
  }
  return t(stateKey(status))
})

/** There is something to remove: a program in any state of its files, or a stored connection. */
const removable = computed(() => {
  const status = props.status
  if (!status) return props.connected
  return (
    !installing(status) &&
    (usable(status) || status.state === 'needs_reinstall' || props.connected)
  )
})

/** The dialog opens after the menu has closed, so the two do not fight over the focus. */
function askRemove() {
  void nextTick(() => {
    removeOpen.value = true
  })
}

function confirmRemove() {
  removeOpen.value = false
  emit('remove')
}
</script>

<template>
  <SettingsRow
    class="@container"
    :title="title"
    :data-testid="`settings-row-cli-tool-${vendor}`"
  >
    <template v-if="status && installing(status)" #backdrop>
      <UiProgress :value="percent" variant="backdrop" />
    </template>
    <template #description>
      <span
        class="block truncate"
        :title="line"
        :data-testid="`cli-tool-line-${vendor}`"
      >
        {{ lead
        }}<template v-if="tail">
          ·
          <span :class="tail.success ? 'text-success' : ''">{{
            tail.text
          }}</span></template
        >
      </span>
    </template>

    <UiButton
      v-if="status && installing(status)"
      type="button"
      variant="outline"
      :data-testid="`cli-tool-cancel-${vendor}`"
      @click="emit('cancel')"
    >
      {{ t('settings.cliTools.cancel') }}
    </UiButton>
    <template v-else>
      <UiButton
        v-if="status && installable(status)"
        type="button"
        variant="outline"
        :data-testid="`cli-tool-install-${vendor}`"
        @click="emit('install')"
      >
        {{ t('settings.cliTools.install') }}
      </UiButton>
      <!-- Wide: every action is a button. Narrow: the same actions are one menu, so the buttons
           never wrap and the row keeps its height. -->
      <div class="hidden items-center gap-2 @3xl:flex">
        <template v-if="status ? usable(status) : connected">
          <UiButton
            v-if="connecting"
            type="button"
            variant="outline"
            @click="emit('showSignIn')"
          >
            {{ t('settings.cliDelegate.showSignIn') }}
          </UiButton>
          <template v-else>
            <UiButton
              type="button"
              variant="outline"
              :disabled="connectBlocked"
              :data-testid="`cli-delegate-connect-${vendor}`"
              @click="emit('connect')"
            >
              {{
                connected
                  ? t('settings.cliDelegate.reconnect')
                  : t('settings.cliDelegate.connect')
              }}
            </UiButton>
            <UiButton
              v-if="connected"
              type="button"
              variant="outline"
              :disabled="refreshing"
              @click="emit('refresh')"
            >
              {{
                refreshing
                  ? t('settings.cliDelegate.refreshing')
                  : t('settings.cliDelegate.refreshModels')
              }}
            </UiButton>
          </template>
          <UiButton
            v-if="status && updatable(status)"
            type="button"
            variant="outline"
            :data-testid="`cli-tool-update-${vendor}`"
            @click="emit('install')"
          >
            {{ t('settings.cliTools.update') }}
          </UiButton>
          <UiButton
            v-else-if="status && revertible(status)"
            type="button"
            variant="outline"
            :data-testid="`cli-tool-revert-${vendor}`"
            @click="emit('revert')"
          >
            {{
              t('settings.cliTools.revert', { version: status.previousVersion })
            }}
          </UiButton>
        </template>
        <DeleteButton
          v-if="removable"
          :label="t('settings.cliTools.remove')"
          :loading="removing"
          :disabled="removing || connecting"
          :data-testid="`cli-tool-remove-${vendor}`"
          @click="removeOpen = true"
        />
      </div>
      <ShadcnDropdownMenu v-if="(status && usable(status)) || removable">
        <ShadcnDropdownMenuTrigger as-child>
          <UiButton
            type="button"
            variant="outline"
            size="icon"
            class="@3xl:hidden"
            :aria-label="t('settings.cliTools.actions')"
            :tooltip="t('settings.cliTools.actions')"
            :data-testid="`cli-tool-actions-${vendor}`"
          >
            <Icon
              name="lucide:ellipsis-vertical"
              class="size-4"
              aria-hidden="true"
            />
          </UiButton>
        </ShadcnDropdownMenuTrigger>
        <ShadcnDropdownMenuContent align="end">
          <template v-if="status ? usable(status) : connected">
            <ShadcnDropdownMenuItem
              v-if="connecting"
              @select="emit('showSignIn')"
            >
              {{ t('settings.cliDelegate.showSignIn') }}
            </ShadcnDropdownMenuItem>
            <template v-else>
              <ShadcnDropdownMenuItem
                :disabled="connectBlocked"
                @select="emit('connect')"
              >
                {{
                  connected
                    ? t('settings.cliDelegate.reconnect')
                    : t('settings.cliDelegate.connect')
                }}
              </ShadcnDropdownMenuItem>
              <ShadcnDropdownMenuItem
                v-if="connected"
                :disabled="refreshing"
                @select="emit('refresh')"
              >
                {{
                  refreshing
                    ? t('settings.cliDelegate.refreshing')
                    : t('settings.cliDelegate.refreshModels')
                }}
              </ShadcnDropdownMenuItem>
            </template>
            <ShadcnDropdownMenuItem
              v-if="status && updatable(status)"
              @select="emit('install')"
            >
              {{ t('settings.cliTools.update') }}
            </ShadcnDropdownMenuItem>
            <ShadcnDropdownMenuItem
              v-else-if="status && revertible(status)"
              @select="emit('revert')"
            >
              {{
                t('settings.cliTools.revert', {
                  version: status.previousVersion,
                })
              }}
            </ShadcnDropdownMenuItem>
          </template>
          <template v-if="removable">
            <ShadcnDropdownMenuSeparator v-if="status && usable(status)" />
            <ShadcnDropdownMenuItem
              class="text-destructive focus:text-destructive"
              :disabled="removing || connecting"
              :data-testid="`cli-tool-remove-menu-${vendor}`"
              @select="askRemove"
            >
              {{ t('settings.cliTools.remove') }}
            </ShadcnDropdownMenuItem>
          </template>
        </ShadcnDropdownMenuContent>
      </ShadcnDropdownMenu>
    </template>
    <UiButton
      type="button"
      variant="ghost"
      size="icon"
      class="size-8 text-muted-foreground"
      :aria-label="t('settings.cliTools.info')"
      :tooltip="t('settings.cliTools.info')"
      v-if="status"
      :data-testid="`cli-tool-info-${vendor}`"
      @click="infoOpen = true"
    >
      <Icon name="lucide:info" class="size-4" aria-hidden="true" />
    </UiButton>

    <UiDrawerModal
      v-if="status"
      v-model:open="infoOpen"
      :title="t('settings.cliTools.title', { name: status.displayName })"
    >
      <template #content>
        <div
          class="flex flex-col gap-2 text-sm"
          :data-testid="`cli-tool-details-${vendor}`"
        >
          <p>{{ stateText }}</p>
          <p v-if="sizeNote">{{ sizeNote }}</p>
          <p
            v-for="(note, index) in notes"
            :key="index"
            :data-testid="`cli-tool-note-${vendor}`"
          >
            {{ note }}
          </p>
          <p
            v-if="status && usable(status)"
            class="text-muted-foreground"
            :data-testid="`cli-tool-isolation-${vendor}`"
          >
            {{ t(isolationKey(status)) }}
          </p>
        </div>
      </template>
    </UiDrawerModal>

    <ShadcnAlertDialog
      :open="removeOpen"
      @update:open="(value: boolean) => (removeOpen = value)"
    >
      <ShadcnAlertDialogContent>
        <ShadcnAlertDialogHeader>
          <ShadcnAlertDialogTitle>{{
            t('settings.cliTools.removeTitle', { name: title })
          }}</ShadcnAlertDialogTitle>
          <ShadcnAlertDialogDescription>
            {{ t('settings.cliTools.removeBody') }}
          </ShadcnAlertDialogDescription>
        </ShadcnAlertDialogHeader>
        <ShadcnAlertDialogFooter>
          <ShadcnAlertDialogCancel>{{
            t('settings.cliTools.cancel')
          }}</ShadcnAlertDialogCancel>
          <UiButton
            variant="destructive"
            :data-testid="`cli-tool-remove-confirm-${vendor}`"
            @click="confirmRemove"
          >
            {{ t('settings.cliTools.remove') }}
          </UiButton>
        </ShadcnAlertDialogFooter>
      </ShadcnAlertDialogContent>
    </ShadcnAlertDialog>
  </SettingsRow>
</template>
