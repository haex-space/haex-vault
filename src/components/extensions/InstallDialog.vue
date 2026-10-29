<script setup lang="ts">
/**
 * Installing an extension from a `.xt` file (spec 017, US1, T048, contracts/permissions.md
 * §Installation): Rust reads and checks the file and shows what it is; every declared permission
 * is listed with a checkbox (on by default; off means "ask when needed"). The extension and its
 * permissions hold in the whole vault, a shell permission only on this device; the dialog says so.
 * An update lists only the new permissions and warns before a downgrade; a bundle of a version the
 * vault already holds with other content replaces it only after a confirmation. Nothing is written
 * before "Installieren".
 *
 * Tools the extension offers to the agent (spec 018) are a group of their own, by title and effect;
 * ticked, the effect decides when a call needs an approval, unticked every call needs one.
 *
 * With `dev` the same dialog loads a project folder in developer mode (US12, FR-064): unsigned,
 * from its development server, every permission for this device only.
 */
import { invoke } from '@tauri-apps/api/core'
import { open as openFile } from '@tauri-apps/plugin-dialog'
import type { DeclaredPermissionView } from '@bindings/DeclaredPermissionView'
import type { InstallPreview } from '@bindings/InstallPreview'
import type { PermissionChoice } from '@bindings/PermissionChoice'
import {
  effectLabelKey,
  splitAgentTools,
  toolTitle,
} from '~/lib/extensions/agentTools'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{ dev?: boolean }>()
const { t, locale } = useI18n()
const { errString } = useErrorString()
const { pickOneAsync } = usePickedFile()

const path = ref<string | null>(null)
const preview = ref<InstallPreview | null>(null)
const choices = ref<Record<string, { granted: boolean }>>({})
const confirmed = ref(false)
const busy = ref(false)
const failure = ref<string | null>(null)

const key = (p: DeclaredPermissionView) => `${p.kind}|${p.action}|${p.target}`

/** On an update only the permissions it adds are put before the user. */
const shown = computed(
  () =>
    preview.value?.existing?.newPermissions ?? preview.value?.declared ?? [],
)
const groups = computed(() => splitAgentTools(shown.value, locale.value))
/** A preview that can be confirmed: a valid bundle, or any project holzi could read. */
const usable = computed(
  () => preview.value !== null && (props.dev || preview.value.signatureValid),
)
/** A downgrade or the replacement of another bundle of the same version: the user confirms. */
const needsConfirmation = computed(
  () =>
    !props.dev &&
    (preview.value?.existing?.isDowngrade === true ||
      preview.value?.replacesSameVersion === true),
)
const blocked = computed(
  () => !usable.value || (needsConfirmation.value && !confirmed.value),
)

function reset() {
  path.value = null
  preview.value = null
  choices.value = {}
  confirmed.value = false
  failure.value = null
}

async function chooseAsync() {
  reset()
  // A project folder (development mode, desktop only) or a bundle file chosen in the dialog
  // (spec 043: a path or an address, passed on unchanged).
  const selected = props.dev
    ? await openFile({ multiple: false, directory: true })
    : await pickOneAsync([
        { name: t('extensions.install.fileType'), extensions: ['xt'] },
      ])
  if (typeof selected !== 'string') {
    open.value = false
    return
  }
  busy.value = true
  try {
    path.value = selected
    preview.value = props.dev
      ? await invoke<InstallPreview>('extension_dev_load', {
          projectPath: selected,
        })
      : await invoke<InstallPreview>('extension_install_preview', {
          file: selected,
        })
    for (const permission of shown.value)
      choices.value[key(permission)] = { granted: true }
  } catch (error) {
    failure.value = errString(error)
  } finally {
    busy.value = false
  }
}

async function installAsync() {
  if (!path.value || blocked.value) return
  busy.value = true
  failure.value = null
  try {
    const accepted: PermissionChoice[] = shown.value.map((p) => ({
      kind: p.kind,
      action: p.action,
      target: p.target,
      granted: choices.value[key(p)]?.granted ?? false,
    }))
    if (props.dev)
      await invoke('extension_dev_confirm', {
        projectPath: path.value,
        accepted,
      })
    else
      await invoke('extension_install', {
        args: {
          file: path.value,
          accepted,
          confirmed: confirmed.value,
        },
      })
    open.value = false
  } catch (error) {
    failure.value = errString(error)
  } finally {
    busy.value = false
  }
}

watch(open, (isOpen) => {
  if (isOpen) void chooseAsync()
  else reset()
})
</script>

<template>
  <UiDrawerModal
    v-model:open="open"
    :title="dev ? t('extensions.dev.loadTitle') : t('extensions.install.title')"
  >
    <template #content>
      <div class="flex flex-col gap-4" data-testid="extensions-install-dialog">
        <p v-if="busy && !preview" class="text-sm text-muted-foreground">
          {{ t('extensions.install.checking') }}
        </p>

        <div
          v-if="preview && !usable"
          class="rounded-xl bg-destructive/10 px-4 py-3 text-sm text-destructive"
          role="alert"
          data-testid="extensions-install-refused"
        >
          <p>{{ t(`extensions.install.errors.${preview.error?.kind}`) }}</p>
          <p v-if="preview.error?.path" class="mt-1 font-mono text-xs">
            {{ preview.error.path }}
          </p>
        </div>

        <template v-if="preview && usable">
          <p v-if="dev" class="rounded-xl bg-warning/15 px-4 py-3 text-sm">
            {{ t('extensions.dev.loadHint') }}
          </p>
          <div class="flex flex-col gap-1">
            <h3 class="text-base font-semibold">
              {{ preview.displayName ?? preview.name }}
              <span class="font-normal text-muted-foreground">{{
                preview.version
              }}</span>
            </h3>
            <p v-if="preview.description" class="text-sm">
              {{ preview.description }}
            </p>
            <p class="text-xs text-muted-foreground">
              {{
                t('extensions.install.publisher', {
                  author: preview.author ?? '—',
                  fingerprint: preview.publisherFingerprint,
                })
              }}
            </p>
            <p v-if="!dev" class="text-xs text-muted-foreground">
              {{ t('extensions.install.vaultWide') }}
            </p>
          </div>

          <p
            v-if="preview.sameNameOtherPublisher"
            class="rounded-xl bg-muted px-4 py-3 text-sm"
          >
            {{ t('extensions.install.sameNameOtherPublisher') }}
          </p>

          <div
            v-if="preview.existing"
            class="rounded-xl bg-muted px-4 py-3 text-sm"
          >
            <p>
              {{
                t('extensions.install.update', {
                  from: preview.existing.version,
                  to: preview.version,
                })
              }}
            </p>
            <label
              v-if="preview.existing.isDowngrade"
              class="mt-2 flex items-center gap-2 text-destructive"
            >
              <ShadcnCheckbox v-model="confirmed" />
              {{ t('extensions.install.confirmDowngrade') }}
            </label>
          </div>

          <div
            v-if="preview.replacesSameVersion && !dev"
            class="rounded-xl bg-muted px-4 py-3 text-sm"
          >
            <p>
              {{
                t('extensions.install.replaceSameVersion', {
                  version: preview.version,
                })
              }}
            </p>
            <label
              v-if="!preview.existing?.isDowngrade"
              class="mt-2 flex items-center gap-2 text-destructive"
            >
              <ShadcnCheckbox v-model="confirmed" />
              {{ t('extensions.install.confirmReplace') }}
            </label>
          </div>

          <SettingsGroup
            v-if="groups.others.length > 0"
            :label="t('extensions.install.permissions')"
          >
            <li
              v-for="permission in groups.others"
              :key="key(permission)"
              class="flex flex-col gap-2 px-4 py-3"
            >
              <label class="flex items-start gap-3">
                <ShadcnCheckbox
                  v-model="choices[key(permission)]!.granted"
                  class="mt-0.5"
                />
                <ExtensionsPermissionSummary
                  :kind="permission.kind"
                  :action="permission.action"
                  :target="permission.target"
                />
              </label>
              <p
                v-if="permission.deviceScoped && !dev"
                class="pl-7 text-xs text-muted-foreground"
              >
                {{ t('extensions.install.deviceOnly') }}
              </p>
            </li>
          </SettingsGroup>

          <div
            v-if="groups.tools.length > 0"
            class="flex flex-col gap-2"
            data-testid="extensions-install-agent-tools"
          >
            <SettingsGroup :label="t('extensions.install.agentTools.title')">
              <li
                v-for="tool in groups.tools"
                :key="key(tool)"
                class="px-4 py-3"
              >
                <label class="flex items-start gap-3">
                  <ShadcnCheckbox
                    v-model="choices[key(tool)]!.granted"
                    class="mt-0.5"
                  />
                  <span class="flex flex-col">
                    <span class="text-sm">
                      {{ toolTitle(tool, locale) }} ({{ tool.target }})
                    </span>
                    <span
                      v-if="tool.supported === false"
                      class="text-xs text-muted-foreground"
                    >
                      {{ t('extensions.install.agentTools.unavailable') }}
                    </span>
                    <span class="text-xs text-muted-foreground">{{
                      t(effectLabelKey(tool.action))
                    }}</span>
                  </span>
                </label>
              </li>
            </SettingsGroup>
            <p class="px-1 text-xs text-muted-foreground">
              {{ t('extensions.install.agentTools.hint') }}
            </p>
          </div>

          <p
            v-if="preview.openers.includes('mail.compose')"
            class="text-xs text-muted-foreground"
            data-testid="extensions-install-opens-mail"
          >
            {{ t('extensions.install.openers.mailCompose') }}
          </p>

          <p
            v-if="preview.unsupportedCategories.length > 0"
            class="text-xs text-muted-foreground"
          >
            {{
              t('extensions.install.unsupported', {
                categories: preview.unsupportedCategories.join(', '),
              })
            }}
          </p>
        </template>

        <p v-if="failure" class="text-sm text-destructive" role="alert">
          {{ failure }}
        </p>

        <div class="flex justify-end gap-2">
          <UiButton variant="outline" @click="open = false">
            {{ t('extensions.install.cancel') }}
          </UiButton>
          <UiButton
            v-if="usable"
            :disabled="busy || blocked"
            data-testid="extensions-install-confirm"
            @click="installAsync"
          >
            {{
              dev ? t('extensions.dev.load') : t('extensions.install.install')
            }}
          </UiButton>
        </div>
      </div>
    </template>
  </UiDrawerModal>
</template>
