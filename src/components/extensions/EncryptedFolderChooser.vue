<script setup lang="ts">
/**
 * The folder chooser of an extension (spec 048 FR-036, contracts/bridge.md
 * `extension_encrypted_folder_choose`): the readable encrypted folders of one cloud storage by name
 * and, in the same view, the question "Lesen erlauben / Lesen und Schreiben erlauben / Ablehnen"
 * with "Erlaubnis merken", which is off by default. It is holzi's own dialog; the extension learns a
 * folder's name only after the user allowed it. Closing it decides nothing. Mounted once in the
 * desktop next to `PermissionRequestDialog.vue`; one chooser at a time.
 */
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { ChooseAnswer } from '@bindings/ChooseAnswer'
import type { ChooseDecision } from '@bindings/ChooseDecision'

interface ChooseRequest {
  requestId: string
  extensionName: string
  storageName: string
  /** The folders lie on this device, not in a cloud storage (spec 048 US7). */
  onDevice?: boolean
  access: 'read' | 'readWrite'
  folders: { folderId: string; name: string }[]
}

const { t } = useI18n()

const queue = ref<ChooseRequest[]>([])
const shown = computed(() => queue.value[0])
const chosen = ref('')
const remember = ref(false)

watch(
  () => shown.value?.requestId,
  () => {
    chosen.value = shown.value?.folders[0]?.folderId ?? ''
    remember.value = false
  },
)

function answer(request: ChooseRequest, choice: ChooseAnswer) {
  queue.value = queue.value.filter((r) => r.requestId !== request.requestId)
  void invoke('extension_encrypted_folder_choose_resolve', {
    requestId: request.requestId,
    answer: choice,
  }).catch((error: unknown) => {
    console.error('[extensions] answering the folder chooser failed', error)
  })
}

function decide(decision: ChooseDecision) {
  const request = shown.value
  if (!request || !chosen.value) return
  answer(request, {
    kind: 'decide',
    folderId: chosen.value,
    decision,
    remember: remember.value,
  })
}

function onUpdateOpen(open: boolean) {
  if (!open && shown.value) answer(shown.value, { kind: 'cancel' })
}

const unlisten: Promise<UnlistenFn>[] = [
  listen<ChooseRequest>('extension-encrypted-folder-choose', (event) => {
    queue.value = [...queue.value, event.payload]
  }),
  listen<{ requestId: string }>(
    'extension-encrypted-folder-choose-ended',
    (event) => {
      queue.value = queue.value.filter(
        (r) => r.requestId !== event.payload.requestId,
      )
    },
  ),
]
onBeforeUnmount(() => {
  for (const pending of unlisten) void pending.then((stop) => stop())
})
</script>

<template>
  <UiDrawerModal
    v-if="shown"
    :open="true"
    :title="
      t('extensions.encryptedChooser.title', { name: shown.extensionName })
    "
    @update:open="onUpdateOpen"
  >
    <template #content>
      <div class="space-y-3" data-testid="extension-encrypted-chooser">
        <p class="text-sm">
          {{
            t(
              shown.onDevice
                ? 'extensions.encryptedChooser.introDevice'
                : 'extensions.encryptedChooser.intro',
              { storage: shown.storageName },
            )
          }}
        </p>
        <p class="text-xs text-muted-foreground">
          {{
            shown.access === 'readWrite'
              ? t('extensions.encryptedChooser.wantsReadWrite')
              : t('extensions.encryptedChooser.wantsRead')
          }}
        </p>
        <p
          v-if="shown.folders.length === 0"
          class="rounded-xl bg-muted px-3 py-2 text-sm"
          data-testid="extension-encrypted-chooser-none"
        >
          {{ t('extensions.encryptedChooser.none') }}
        </p>
        <ShadcnRadioGroup
          v-else
          class="gap-2"
          :model-value="chosen"
          @update:model-value="chosen = String($event)"
        >
          <label
            v-for="folder in shown.folders"
            :key="folder.folderId"
            class="flex items-center gap-2 text-sm"
            :data-testid="`extension-encrypted-chooser-folder-${folder.name}`"
          >
            <ShadcnRadioGroupItem :value="folder.folderId" />
            <Icon name="lucide:folder-lock" class="size-4 shrink-0" />
            {{ folder.name }}
          </label>
        </ShadcnRadioGroup>
        <label class="flex items-center gap-2 text-sm">
          <ShadcnCheckbox
            v-model="remember"
            data-testid="extension-encrypted-chooser-remember"
          />
          {{ t('extensions.encryptedChooser.remember') }}
        </label>
        <p class="text-xs text-muted-foreground">
          {{
            remember
              ? t('extensions.permissionRequest.scopeVault')
              : t('extensions.encryptedChooser.untilReload')
          }}
        </p>
      </div>
    </template>
    <template #footer>
      <div class="flex flex-wrap justify-end gap-2">
        <UiButton
          type="button"
          size="sm"
          variant="outline"
          data-testid="extension-encrypted-chooser-deny"
          :disabled="!chosen"
          @click="decide('deny')"
        >
          {{ t('extensions.encryptedChooser.deny') }}
        </UiButton>
        <UiButton
          type="button"
          size="sm"
          variant="outline"
          data-testid="extension-encrypted-chooser-read"
          :disabled="!chosen"
          @click="decide('read')"
        >
          {{ t('extensions.encryptedChooser.allowRead') }}
        </UiButton>
        <UiButton
          v-if="shown.access === 'readWrite'"
          type="button"
          size="sm"
          data-testid="extension-encrypted-chooser-read-write"
          :disabled="!chosen"
          @click="decide('readWrite')"
        >
          {{ t('extensions.encryptedChooser.allowReadWrite') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
