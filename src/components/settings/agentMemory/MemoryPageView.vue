<script setup lang="ts">
/**
 * One page of the agents' shared memory (spec 097, US3, FR-024, FR-025): its text as Markdown, where
 * `[[Title]]` opens that page and a pointer to no page offers to create it; the pages it points to
 * and those that point to it; who wrote it and who changed it last. The user changes title,
 * description and text, or deletes the page, without a question; every write is held to the
 * limits and the check of the backend, which names why it refused. `new` as the page id is an
 * empty page (a `title` in the query fills its title).
 */
import DOMPurify from 'dompurify'
import { marked } from 'marked'
import { toast } from 'vue-sonner'
import { onVaultTablesChanged } from '~/composables/useVaultData'
import {
  AGENT_MEMORY_TABLES,
  charCount,
  MEMORY_FIELD_MAX,
  memoryRefusal,
  withPointerLinks,
  type MemoryField,
} from '~/lib/agentMemory/pages'
import type { AgentMemoryPage } from '~/types/bindings/AgentMemoryPage'

const { t, d } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const router = useTabRouter()
const store = useAgentMemoryStore()

const pageId = computed(() => String(router.route.params.pageId ?? ''))
const isNew = computed(() => pageId.value === 'new')
const page = ref<AgentMemoryPage | null>(null)
const loaded = ref(false)
const editing = ref(false)
const busy = ref(false)
const confirmDelete = ref(false)
const draft = reactive({ title: '', description: '', body: '' })
/** Set from deleting on: the deletion's change event would reload a page that is gone. */
let leaving = false

function startDraft(from: AgentMemoryPage | null) {
  const title = router.route.query.title
  draft.title = from?.title ?? (typeof title === 'string' ? title : '')
  draft.description = from?.description ?? ''
  draft.body = from?.body ?? ''
}

async function reloadAsync() {
  confirmDelete.value = false
  if (isNew.value) {
    page.value = null
    editing.value = true
    startDraft(null)
    loaded.value = true
    return
  }
  try {
    page.value = await store.pageAsync(pageId.value)
    if (!editing.value) startDraft(page.value)
  } catch (error) {
    page.value = null
    if (!leaving && !memoryRefusal(error)) toast.error(errString(error))
  } finally {
    loaded.value = true
  }
}

function showError(error: unknown) {
  const refusal = memoryRefusal(error)
  if (!refusal) {
    toast.error(errString(error))
    return
  }
  const field = refusal.field
    ? t(`settings.agentMemory.fields.${refusal.field}`)
    : ''
  toast.error(t(refusal.key, { field }))
}

function counter(field: MemoryField): string {
  return t('settings.agentMemory.counter', {
    count: charCount(draft[field]),
    max: MEMORY_FIELD_MAX[field],
  })
}

const rendered = computed(() =>
  page.value
    ? DOMPurify.sanitize(
        marked.parse(withPointerLinks(page.value.body, page.value.pointsTo), {
          async: false,
          breaks: true,
        }),
      )
    : '',
)

function author(kind: string | null, name: string | null): string {
  if (kind === 'user') return t('settings.agentMemory.byUser')
  return name || t('settings.agentMemory.unknownAuthor')
}

function when(at: number): string {
  return d(new Date(at), { dateStyle: 'medium', timeStyle: 'short' })
}

const written = computed(() => {
  const current = page.value
  if (!current) return ''
  return t('settings.agentMemory.written', {
    name: author(current.authorKind, current.authorName),
    device: current.deviceName ?? t('settings.agentMemory.unknownDevice'),
    at: when(current.createdAt),
  })
})

const changed = computed(() => {
  const current = page.value
  if (!current || current.changedAt === null) return ''
  return t('settings.agentMemory.changed', {
    name: author(current.changedKind, current.changedName),
    device:
      current.changedDeviceName ?? t('settings.agentMemory.unknownDevice'),
    at: when(current.changedAt),
  })
})

function openPage(id: string) {
  router.push(`/ai/memory/${id}`)
}

function createPage(title: string) {
  router.push({ path: '/ai/memory/new', query: { title } })
}

/** A pointer in the rendered text was clicked: open its page, or create the missing one. */
function onTextClick(event: MouseEvent) {
  const target = event.target as HTMLElement | null
  const link = target?.closest<HTMLElement>('[data-memory-page]')
  if (link?.dataset.memoryPage) {
    event.preventDefault()
    openPage(link.dataset.memoryPage)
    return
  }
  const missing = target?.closest<HTMLElement>('[data-memory-missing]')
  if (missing?.dataset.memoryMissing) createPage(missing.dataset.memoryMissing)
}

async function saveAsync() {
  if (busy.value) return
  busy.value = true
  try {
    if (isNew.value) {
      const created = await store.addAsync({ ...draft })
      editing.value = false
      router.push(`/ai/memory/${created.id}`)
      return
    }
    page.value = await store.changeAsync({ id: pageId.value, ...draft })
    editing.value = false
  } catch (error) {
    showError(error)
  } finally {
    busy.value = false
  }
}

function cancelEdit() {
  if (isNew.value) {
    router.push('/ai/memory')
    return
  }
  editing.value = false
  startDraft(page.value)
}

async function deleteAsync() {
  leaving = true
  try {
    await store.removeAsync(pageId.value)
  } catch (error) {
    leaving = false
    showError(error)
    return
  }
  router.push('/ai/memory')
}

onMounted(reloadAsync)
onBeforeUnmount(() => {
  leaving = true
})
watch(pageId, () => {
  editing.value = false
  void reloadAsync()
})
onVaultTablesChanged(AGENT_MEMORY_TABLES, () => {
  if (!isNew.value && !leaving) return reloadAsync()
})
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="agent-memory-page">
    <p
      v-if="loaded && !isNew && !page"
      class="px-1 text-sm text-muted-foreground"
      role="status"
      data-testid="agent-memory-page-gone"
    >
      {{ t('settings.agentMemory.gone') }}
    </p>

    <SettingsGroup v-if="editing">
      <div class="flex flex-col gap-3 p-4">
        <UiInput
          id="agent-memory-title"
          v-model="draft.title"
          type="text"
          :label="t('settings.agentMemory.fields.title')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          data-testid="agent-memory-title"
        />
        <p class="-mt-2 text-right text-xs text-muted-foreground">
          {{ counter('title') }}
        </p>
        <UiInput
          id="agent-memory-description"
          v-model="draft.description"
          type="text"
          :label="t('settings.agentMemory.fields.description')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          data-testid="agent-memory-description"
        />
        <p class="-mt-2 text-right text-xs text-muted-foreground">
          {{ counter('description') }}
        </p>
        <UiTextarea
          id="agent-memory-body"
          v-model="draft.body"
          :label="t('settings.agentMemory.fields.body')"
          label-bg="var(--muted)"
          rows="12"
          data-testid="agent-memory-body"
        />
        <p class="-mt-2 text-xs text-muted-foreground">
          {{ t('settings.agentMemory.bodyHint') }} · {{ counter('body') }}
        </p>
        <div class="flex flex-wrap justify-end gap-2">
          <UiButton type="button" variant="outline" @click="cancelEdit">
            {{ t('settings.agentMemory.cancel') }}
          </UiButton>
          <UiButton
            type="button"
            :loading="busy"
            :disabled="!draft.title.trim() || !draft.description.trim()"
            data-testid="agent-memory-save"
            @click="saveAsync"
          >
            {{ t('settings.agentMemory.save') }}
          </UiButton>
        </div>
      </div>
    </SettingsGroup>

    <template v-if="page && !editing">
      <SettingsGroup>
        <SettingsRow
          :title="page.title"
          :description="page.description"
          data-testid="agent-memory-page-title"
        >
          <UiButton
            type="button"
            variant="outline"
            data-testid="agent-memory-edit"
            @click="editing = true"
          >
            {{ t('settings.agentMemory.edit') }}
          </UiButton>
        </SettingsRow>
        <SettingsRow
          :title="written"
          :description="changed || undefined"
          data-testid="agent-memory-origin"
        />
        <SettingsRow
          v-if="page.sameTitle"
          icon="lucide:copy"
          :title="t('settings.agentMemory.sameTitle')"
          :description="t('settings.agentMemory.sameTitleHint')"
        />
      </SettingsGroup>

      <SettingsGroup>
        <!-- eslint-disable vue/no-v-html -->
        <div
          class="chat-markdown memory-text p-4 text-sm"
          data-testid="agent-memory-text"
          @click="onTextClick"
          v-html="rendered"
        />
        <!-- eslint-enable vue/no-v-html -->
        <p v-if="!page.body.trim()" class="p-4 text-sm text-muted-foreground">
          {{ t('settings.agentMemory.noText') }}
        </p>
      </SettingsGroup>

      <SettingsGroup :label="t('settings.agentMemory.pointsTo')">
        <template v-for="pointer in page.pointsTo" :key="pointer.title">
          <SettingsRow
            v-if="pointer.id"
            icon="lucide:file-text"
            :title="pointer.title"
            :to="`/ai/memory/${pointer.id}`"
            data-testid="agent-memory-points-to"
          />
          <SettingsRow
            v-else
            icon="lucide:file-question"
            :title="pointer.title"
            :description="t('settings.agentMemory.missing')"
            data-testid="agent-memory-points-to-missing"
          >
            <UiButton
              type="button"
              variant="outline"
              @click="createPage(pointer.title)"
            >
              {{ t('settings.agentMemory.createMissing') }}
            </UiButton>
          </SettingsRow>
        </template>
        <p
          v-if="page.pointsTo.length === 0"
          class="p-4 text-sm text-muted-foreground"
        >
          {{ t('settings.agentMemory.noPointers') }}
        </p>
      </SettingsGroup>

      <SettingsGroup :label="t('settings.agentMemory.pointedFrom')">
        <SettingsRow
          v-for="pointer in page.pointedFrom"
          :key="pointer.title"
          icon="lucide:file-text"
          :title="pointer.title"
          :to="pointer.id ? `/ai/memory/${pointer.id}` : undefined"
          data-testid="agent-memory-pointed-from"
        />
        <p
          v-if="page.pointedFrom.length === 0"
          class="p-4 text-sm text-muted-foreground"
        >
          {{ t('settings.agentMemory.noPointers') }}
        </p>
      </SettingsGroup>

      <SettingsGroup>
        <div class="flex flex-wrap items-center justify-end gap-2 p-4">
          <template v-if="confirmDelete">
            <span class="text-sm">{{
              t('settings.agentMemory.deleteConfirm')
            }}</span>
            <UiButton
              type="button"
              variant="outline"
              @click="confirmDelete = false"
            >
              {{ t('settings.agentMemory.cancel') }}
            </UiButton>
            <UiButton
              type="button"
              variant="destructive"
              data-testid="agent-memory-delete-confirm"
              @click="deleteAsync"
            >
              {{ t('settings.agentMemory.delete') }}
            </UiButton>
          </template>
          <UiButton
            v-else
            type="button"
            variant="outline"
            data-testid="agent-memory-delete"
            @click="confirmDelete = true"
          >
            {{ t('settings.agentMemory.delete') }}
          </UiButton>
        </div>
      </SettingsGroup>
    </template>
  </section>
</template>

<style scoped>
.memory-text :deep(.memory-missing) {
  color: var(--muted-foreground);
  text-decoration: underline dotted;
  cursor: pointer;
}
</style>
