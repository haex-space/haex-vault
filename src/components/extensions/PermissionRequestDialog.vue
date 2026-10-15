<script setup lang="ts">
/**
 * The question of an extension for a permission (spec 017, US3, T069, contracts/permissions.md):
 * one at a time, "Erlauben" or "Verweigern", "Merken" to keep the answer, which then holds on every
 * own device, for the shell only on this one. Closing the dialog cancels; it never denies. Mounted once in the
 * desktop, following the controlled dialog of `chat/PermissionPrompt.vue`. For an encrypted folder
 * (spec 048 FR-036) it names the cloud storage and the folder, and "Erlaubnis merken" starts off:
 * without it the answer holds until the extension reloads.
 */
const { t } = useI18n()
const { errString } = useErrorString()
const store = useExtensionPermissionsStore()

/** Whether "remember" starts ticked: not for an encrypted folder (spec 048 FR-036). */
const rememberByDefault = () => store.shown?.kind !== 'encryptedFolder'
const remember = ref(rememberByDefault())
const failure = ref<string | null>(null)
const folderKind = computed(() => store.shown?.kind === 'encryptedFolder')

watch(
  () => store.shown?.requestId,
  () => {
    remember.value = rememberByDefault()
    failure.value = null
  },
)

async function answerAsync(decision: 'allow' | 'deny') {
  const question = store.shown
  if (!question) return
  try {
    await store.answerAsync(question.requestId, decision, remember.value)
  } catch (error) {
    failure.value = errString(error)
  }
}

function onUpdateOpen(open: boolean) {
  if (!open && store.shown) store.cancel(store.shown.requestId)
}
</script>

<template>
  <UiDrawerModal
    v-if="store.shown"
    :open="true"
    :title="
      t('extensions.permissionRequest.title', {
        name: store.shown.displayName,
      })
    "
    @update:open="onUpdateOpen"
  >
    <template #content>
      <div class="space-y-3" data-testid="extension-permission-request">
        <ExtensionsPermissionSummary
          :kind="store.shown.kind"
          :action="store.shown.action"
          :target="store.shown.target"
          :folder="store.shown.encryptedFolder"
        />
        <p
          v-if="store.shown.byAgent"
          class="text-xs text-muted-foreground"
          data-testid="extension-permission-by-agent"
        >
          {{ t('extensions.permissionRequest.byAgent') }}
        </p>
        <p
          v-if="!store.shown.declared && !folderKind"
          class="text-xs text-muted-foreground"
        >
          {{ t('extensions.permissionRequest.notDeclared') }}
        </p>
        <p
          v-if="store.shown.targetMissing"
          class="rounded-xl bg-muted px-3 py-2 text-sm"
        >
          {{ t('extensions.permissionRequest.targetMissing') }}
        </p>
        <p
          v-if="store.shown.kind === 'shell'"
          class="rounded-xl bg-destructive/10 px-3 py-2 text-sm text-destructive"
          role="alert"
        >
          {{ t('extensions.permissionRequest.shellWarning') }}
        </p>
        <label class="flex items-center gap-2 text-sm">
          <ShadcnCheckbox
            v-model="remember"
            data-testid="extension-permission-remember"
          />
          {{
            folderKind
              ? t('extensions.encryptedChooser.remember')
              : t('extensions.permissionRequest.remember')
          }}
        </label>
        <p v-if="folderKind && !remember" class="text-xs text-muted-foreground">
          {{ t('extensions.encryptedChooser.untilReload') }}
        </p>
        <p v-if="remember" class="text-xs text-muted-foreground">
          {{
            store.shown.deviceScoped
              ? t('extensions.permissionRequest.scopeDevice')
              : t('extensions.permissionRequest.scopeVault')
          }}
        </p>
        <p v-if="failure" class="text-sm text-destructive" role="alert">
          {{ failure }}
        </p>
      </div>
    </template>
    <template #footer>
      <div class="flex justify-end gap-2">
        <UiButton
          size="sm"
          variant="outline"
          data-testid="extension-permission-deny"
          @click="answerAsync('deny')"
        >
          {{ t('extensions.permissionRequest.deny') }}
        </UiButton>
        <UiButton
          v-if="!store.shown.targetMissing"
          size="sm"
          data-testid="extension-permission-allow"
          @click="answerAsync('allow')"
        >
          {{ t('extensions.permissionRequest.allow') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
