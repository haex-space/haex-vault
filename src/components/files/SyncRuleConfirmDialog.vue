<script setup lang="ts">
/**
 * The confirmation before an agent creates, changes or deletes a sync rule (spec 061 FR-035,
 * research R11): it names the rule with its places, direction, delete mode and interval, and for a
 * change the old value next to each new one. A rule that would store files of an encrypted place
 * decrypted says so. Only "Erlauben" lets it through; there is nothing to remember, and without an
 * answer in 120 s Rust refuses. Confirmations that arrive together wait in line.
 */
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { Sources } from '@bindings/Sources'
import type { SyncRuleConfirmRequest } from '@bindings/SyncRuleConfirmRequest'
import type { SyncRuleDraft } from '@bindings/SyncRuleDraft'
import { placeLabel } from '~/lib/files/syncRules'

const { t } = useI18n()
const { sourcesAsync } = useFiles()
const queue = ref<SyncRuleConfirmRequest[]>([])
const current = computed(() => queue.value[0])
const sources = ref<Sources | null>(null)
const CONFIRM_TIMEOUT_MS = 120_000
const timeouts = new Map<string, ReturnType<typeof setTimeout>>()
let unlisten: UnlistenFn | undefined

function remove(requestId: string) {
  clearTimeout(timeouts.get(requestId))
  timeouts.delete(requestId)
  queue.value = queue.value.filter((request) => request.requestId !== requestId)
}

onMounted(async () => {
  try {
    unlisten = await listen<SyncRuleConfirmRequest>(
      'sync-rule-confirm-request',
      (event) => {
        const request = event.payload
        queue.value.push(request)
        timeouts.set(
          request.requestId,
          setTimeout(() => remove(request.requestId), CONFIRM_TIMEOUT_MS),
        )
        // Storages and devices by their names; without them a place shows its path.
        sourcesAsync()
          .then((loaded) => (sources.value = loaded))
          .catch(() => {})
      },
    )
  } catch (error) {
    console.error('[files] listening for sync rule confirmations failed', error)
  }
})

onBeforeUnmount(() => {
  unlisten?.()
  for (const timeout of timeouts.values()) clearTimeout(timeout)
  timeouts.clear()
})

async function answerAsync(confirmed: boolean) {
  const request = current.value
  if (!request) return
  remove(request.requestId)
  await invoke('sync_rule_confirm_answer', {
    requestId: request.requestId,
    confirmed,
  }).catch((error: unknown) => {
    console.error('[files] answering the sync rule confirmation failed', error)
  })
}

/** The values the dialog lists, as text. */
function shown(draft: SyncRuleDraft) {
  const unknown = t('files.syncRules.unknownSource')
  return {
    placeA: placeLabel(draft.placeA, sources.value, unknown),
    placeB: placeLabel(draft.placeB, sources.value, unknown),
    direction: t(`files.syncRules.direction.${draft.direction}`),
    deleteMode: t(`files.syncRules.deleteMode.${draft.deleteMode}`),
    interval: t(
      'files.syncRules.confirm.minutes',
      Math.round(draft.intervalSecs / 60),
    ),
  }
}

const FIELDS = [
  ['placeA', 'files.syncRules.placeA'],
  ['placeB', 'files.syncRules.placeB'],
  ['direction', 'files.syncRules.direction.label'],
  ['deleteMode', 'files.syncRules.deleteMode.label'],
  ['interval', 'files.syncRules.confirm.interval'],
] as const

const rows = computed(() => {
  const request = current.value
  if (!request) return []
  const now = shown(request.rule)
  const before = request.before ? shown(request.before) : null
  return FIELDS.map(([key, label]) => ({
    key,
    label: t(label),
    value: now[key],
    before: before && before[key] !== now[key] ? before[key] : null,
  }))
})
</script>

<template>
  <ShadcnAlertDialog
    :open="current !== undefined"
    @update:open="(open: boolean) => !open && answerAsync(false)"
  >
    <ShadcnAlertDialogContent
      v-if="current"
      data-testid="files-sync-rule-confirm"
    >
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>
          {{ t(`files.syncRules.confirm.title.${current.action}`) }}
        </ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>
          {{
            t(`files.syncRules.confirm.body.${current.action}`, {
              name: current.rule.name,
            })
          }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <dl
        class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 text-sm"
        data-testid="files-sync-rule-confirm-rule"
      >
        <template v-for="row in rows" :key="row.key">
          <dt class="text-muted-foreground">{{ row.label }}</dt>
          <dd class="min-w-0 break-words">
            <span
              v-if="row.before"
              class="text-muted-foreground line-through"
              :data-testid="`files-sync-rule-confirm-before-${row.key}`"
              >{{ row.before }}</span
            >
            <span v-if="row.before"> → </span>
            <span :data-testid="`files-sync-rule-confirm-${row.key}`">{{
              row.value
            }}</span>
          </dd>
        </template>
      </dl>
      <p
        v-if="current.action !== 'delete' && current.rule.allowDecrypted"
        class="text-sm text-warning"
        data-testid="files-sync-rule-confirm-decrypted"
      >
        {{ t('files.syncRules.confirm.decrypted') }}
      </p>
      <ShadcnAlertDialogFooter class="flex-wrap gap-2">
        <ShadcnAlertDialogCancel data-testid="files-sync-rule-confirm-deny">
          {{ t('files.syncRules.confirm.deny') }}
        </ShadcnAlertDialogCancel>
        <UiButton
          data-testid="files-sync-rule-confirm-allow"
          @click="answerAsync(true)"
        >
          {{ t('files.syncRules.confirm.allow') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
