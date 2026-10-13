<script setup lang="ts">
/**
 * The question when the agent of the chat reaches for a storage it holds no permission for (spec
 * 044 FR-031a, research R14), or for an encrypted folder in it (spec 048 FR-030a, FR-031): read,
 * read and write, or deny. A folder's question names the model and whether it runs on this device
 * or in the cloud; its answer is kept only with "Erlaubnis merken", and for cloud models only with
 * "auch für Cloud-Modelle" too. Rust stores the answer; without an answer in 60 s it refuses this
 * one call. Questions that arrive together wait in line.
 */
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { FilesAgentChoice } from '@bindings/FilesAgentChoice'
import type { FilesAgentPermissionRequest } from '@bindings/FilesAgentPermissionRequest'

const { t } = useI18n()
const queue = ref<FilesAgentPermissionRequest[]>([])
const current = computed(() => queue.value[0])
const remember = ref(false)
const alsoCloud = ref(false)
watch(
  () => current.value?.requestId,
  () => {
    remember.value = false
    alsoCloud.value = false
  },
)
let unlisten: UnlistenFn | undefined
const requestTimeouts = new Map<string, ReturnType<typeof setTimeout>>()
const PERMISSION_REQUEST_TIMEOUT_MS = 60_000

const emit = defineEmits<{
  ready: []
}>()

function removeRequest(requestId: string): void {
  const timeout = requestTimeouts.get(requestId)
  if (timeout) clearTimeout(timeout)
  requestTimeouts.delete(requestId)
  queue.value = queue.value.filter((request) => request.requestId !== requestId)
}

onMounted(async () => {
  try {
    unlisten = await listen<FilesAgentPermissionRequest>(
      'files-agent-permission-request',
      (event) => {
        const request = event.payload
        queue.value.push(request)
        requestTimeouts.set(
          request.requestId,
          setTimeout(
            () => removeRequest(request.requestId),
            PERMISSION_REQUEST_TIMEOUT_MS,
          ),
        )
      },
    )
  } catch (error) {
    console.error(
      '[files] listening for agent permission requests failed',
      error,
    )
  } finally {
    emit('ready')
  }
})

onBeforeUnmount(() => {
  unlisten?.()
  for (const timeout of requestTimeouts.values()) clearTimeout(timeout)
  requestTimeouts.clear()
})

async function answerAsync(choice: FilesAgentChoice): Promise<void> {
  const request = queue.value.shift()
  if (!request) return
  const timeout = requestTimeouts.get(request.requestId)
  if (timeout) clearTimeout(timeout)
  requestTimeouts.delete(request.requestId)
  const folder = request.folder !== undefined
  await invoke('files_agent_permission_answer', {
    args: {
      requestId: request.requestId,
      choice,
      remember: folder && remember.value,
      alsoCloud: folder && remember.value && alsoCloud.value,
    },
  }).catch((error: unknown) => {
    console.error('[files] answering the storage question failed', error)
  })
}
</script>

<template>
  <ShadcnAlertDialog
    :open="current !== undefined"
    @update:open="(open: boolean) => !open && answerAsync('deny')"
  >
    <ShadcnAlertDialogContent
      v-if="current"
      data-testid="files-agent-permission"
    >
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>
          {{
            current.folder
              ? t('files.agentPermission.folder.title')
              : t('files.agentPermission.title')
          }}
        </ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription v-if="current.folder">
          {{
            t(`files.agentPermission.folder.${current.wants}`, {
              name: current.folder.name,
              storage: current.storageName,
            })
          }}
          {{
            t(
              current.folder.cloud
                ? 'files.agentPermission.folder.cloud'
                : 'files.agentPermission.folder.local',
              { model: current.folder.model },
            )
          }}
        </ShadcnAlertDialogDescription>
        <ShadcnAlertDialogDescription v-else>
          {{
            t(`files.agentPermission.${current.wants}`, {
              name: current.storageName,
            })
          }}
          {{ t('files.agentPermission.hint') }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <div v-if="current.folder" class="flex flex-col gap-2">
        <label class="flex items-center gap-2 text-sm">
          <ShadcnCheckbox
            v-model="remember"
            data-testid="files-agent-permission-remember"
          />
          {{ t('files.agentPermission.folder.remember') }}
        </label>
        <label
          v-if="remember && current.folder.cloud"
          class="flex items-center gap-2 text-sm"
        >
          <ShadcnCheckbox
            v-model="alsoCloud"
            data-testid="files-agent-permission-also-cloud"
          />
          {{ t('files.agentPermission.folder.alsoCloud') }}
        </label>
        <p class="text-xs text-muted-foreground">
          {{
            remember
              ? alsoCloud || !current.folder.cloud
                ? t('files.agentPermission.folder.kept')
                : t('files.agentPermission.folder.keptLocal')
              : t('files.agentPermission.folder.thisTask')
          }}
        </p>
      </div>
      <ShadcnAlertDialogFooter class="flex-wrap gap-2">
        <ShadcnAlertDialogCancel data-testid="files-agent-permission-deny">
          {{ t('files.agentPermission.deny') }}
        </ShadcnAlertDialogCancel>
        <UiButton
          variant="outline"
          data-testid="files-agent-permission-read"
          @click="answerAsync('read')"
        >
          {{ t('files.agentPermission.allowRead') }}
        </UiButton>
        <UiButton
          data-testid="files-agent-permission-read-write"
          @click="answerAsync('readWrite')"
        >
          {{ t('files.agentPermission.allowReadWrite') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
