<script setup lang="ts">
/**
 * The transfer bar (spec 044 FR-019, FR-021): every running or failed transfer with its progress,
 * cancel, and for a failed one the reason and "try again". A name conflict, or files about to
 * leave an encrypted folder (spec 048 FR-019), opens its question in the tab that started the
 * transfer; any other tab offers a button for it.
 */
import { toast } from 'vue-sonner'
import type { TransferAnswer } from '@bindings/TransferAnswer'
import type { TransferView } from '~/stores/filesTransfers'
import { formatFileSize } from '~/lib/passwords/format'

const props = defineProps<{ tabId: string }>()

const { t } = useI18n()
const { errString } = useErrorString()
const store = useFilesTransfersStore()

/** A conflict the user asked to answer here, though another tab started the transfer. */
const answering = ref<number | null>(null)

function waits(transfer: TransferView): boolean {
  return transfer.conflict !== null || transfer.leaving !== null
}

const asking = computed(() =>
  store.transfers.find(
    (transfer) =>
      waits(transfer) &&
      (transfer.tabId === props.tabId || transfer.key === answering.value),
  ),
)

function label(transfer: TransferView): string {
  return t(`files.transfer.${transfer.op}`)
}

function share(transfer: TransferView): number {
  const progress = transfer.progress
  if (!progress) return 0
  if (progress.bytesTotal > 0) return progress.bytesDone / progress.bytesTotal
  return progress.itemsTotal > 0 ? progress.itemsDone / progress.itemsTotal : 0
}

function detail(transfer: TransferView): string {
  const progress = transfer.progress
  if (!progress) return ''
  const items = `${progress.itemsDone} / ${progress.itemsTotal}`
  return progress.bytesTotal > 0
    ? `${items} · ${formatFileSize(progress.bytesDone)} / ${formatFileSize(progress.bytesTotal)}`
    : items
}

function failure(transfer: TransferView): string {
  const code = transfer.error?.code
  return code ? t(`files.error.${code}`) : (transfer.error?.message ?? '')
}

async function run(work: () => Promise<void>) {
  try {
    await work()
  } catch (cause) {
    toast.error(errString(cause))
  }
}

function answer(choice: TransferAnswer, forAll: boolean) {
  const transfer = asking.value
  if (!transfer) return
  answering.value = null
  void run(() => store.answerAsync(transfer.key, choice, forAll))
}

function cancelAsking() {
  const transfer = asking.value
  answering.value = null
  if (transfer) void run(() => store.cancelAsync(transfer.key))
}
</script>

<template>
  <div
    v-if="store.transfers.length"
    class="flex shrink-0 flex-col gap-1 border-t bg-background px-3 py-2"
    data-testid="files-transfers"
  >
    <div
      v-for="transfer in store.transfers"
      :key="transfer.key"
      class="flex items-center gap-3 text-sm"
      :data-testid="`files-transfer-${transfer.state}`"
    >
      <Icon
        :name="
          transfer.state === 'failed' ? 'lucide:circle-alert' : 'lucide:loader'
        "
        class="size-4 shrink-0"
        :class="
          transfer.state === 'failed'
            ? 'text-destructive'
            : 'animate-spin text-muted-foreground'
        "
      />
      <div class="flex min-w-0 flex-1 flex-col gap-1">
        <div class="flex min-w-0 items-baseline gap-2">
          <span class="shrink-0 font-medium">{{ label(transfer) }}</span>
          <span
            v-if="transfer.state === 'failed'"
            class="truncate text-destructive"
            >{{ failure(transfer) }}</span
          >
          <span v-else class="truncate text-xs text-muted-foreground">{{
            detail(transfer)
          }}</span>
        </div>
        <div
          v-if="transfer.state === 'running'"
          class="h-1 overflow-hidden rounded bg-muted"
        >
          <div
            class="h-full bg-primary transition-[width]"
            :style="{ width: `${Math.round(share(transfer) * 100)}%` }"
          />
        </div>
      </div>
      <UiButton
        v-if="waits(transfer) && transfer.tabId !== tabId"
        size="sm"
        variant="outline"
        @click="answering = transfer.key"
      >
        {{ t('files.conflict.answer') }}
      </UiButton>
      <UiButton
        v-if="transfer.state === 'failed'"
        size="sm"
        variant="outline"
        data-testid="files-transfer-retry"
        @click="run(() => store.retryAsync(transfer.key))"
      >
        {{ t('files.transfer.retry') }}
      </UiButton>
      <UiButton
        variant="ghost"
        size="icon"
        :aria-label="t('files.transfer.cancel')"
        :tooltip="t('files.transfer.cancel')"
        data-testid="files-transfer-cancel"
        @click="run(() => store.cancelAsync(transfer.key))"
      >
        <Icon name="lucide:x" class="size-4" />
      </UiButton>
    </div>
    <FilesConflictDialog
      :open="!!asking"
      :name="asking?.conflict ?? ''"
      :leaving="asking?.leaving ?? null"
      @answer="answer"
      @cancel="cancelAsking"
    />
  </div>
</template>
