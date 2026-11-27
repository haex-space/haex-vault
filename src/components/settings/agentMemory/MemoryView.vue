<script setup lang="ts">
/**
 * "KI → Gedächtnis" (spec 097, US3, FR-024): every page of the agents' shared memory with its
 * description, who wrote it (the user or the model by its name), on which device and when, the
 * page changed last first, and a search over title, description and text. A page opens from its
 * row; "Seite hinzufügen" opens an empty page. The list follows changes of agents and of other
 * devices by itself.
 */
import { toast } from 'vue-sonner'
import type { AgentMemoryListItem } from '~/types/bindings/AgentMemoryListItem'

const { t, d } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const router = useTabRouter()
const store = useAgentMemoryStore()

const query = ref('')
const loaded = ref(false)
let searchTimer: ReturnType<typeof setTimeout> | undefined

async function searchAsync() {
  try {
    await store.searchAsync(query.value)
  } catch (error) {
    toast.error(errString(error))
  } finally {
    loaded.value = true
  }
}

function onQuery() {
  clearTimeout(searchTimer)
  searchTimer = setTimeout(searchAsync, 200)
}

const pages = computed(() => store.list?.pages ?? [])

function author(page: AgentMemoryListItem): string {
  return page.authorKind === 'user'
    ? t('settings.agentMemory.byUser')
    : page.authorName || t('settings.agentMemory.unknownAuthor')
}

function describe(page: AgentMemoryListItem): string {
  const at = page.changedAt ?? page.createdAt
  const origin = [
    page.changedName ?? author(page),
    page.deviceName ?? t('settings.agentMemory.unknownDevice'),
    d(new Date(at), { dateStyle: 'medium', timeStyle: 'short' }),
  ].join(' · ')
  const parts = [page.description, origin]
  if (page.sameTitle) parts.push(t('settings.agentMemory.sameTitle'))
  return parts.join(' — ')
}

function addPage() {
  router.push('/ai/memory/new')
}

onMounted(searchAsync)
onBeforeUnmount(() => clearTimeout(searchTimer))
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="agent-memory">
    <p class="px-1 text-sm text-muted-foreground">
      {{ t('settings.agentMemory.intro') }}
    </p>

    <SettingsGroup>
      <SettingsRow
        :title="t('settings.agentMemory.search')"
        :description="
          store.list
            ? t('settings.agentMemory.count', {
                count: store.list.count,
                limit: store.list.limit,
              })
            : undefined
        "
        label-for="agent-memory-search"
      >
        <SettingsField size="xl">
          <div class="flex items-center gap-2">
            <UiInput
              id="agent-memory-search"
              v-model="query"
              type="search"
              :placeholder="t('settings.agentMemory.searchPlaceholder')"
              :labels="fieldLabels.input.value"
              data-testid="agent-memory-search"
              @update:model-value="onQuery"
            />
            <UiButtonAdd
              :label="t('settings.agentMemory.add')"
              variant="outline"
              data-testid="agent-memory-add"
              @click="addPage"
            />
          </div>
        </SettingsField>
      </SettingsRow>
    </SettingsGroup>

    <p
      v-if="loaded && pages.length === 0"
      class="px-1 text-sm text-muted-foreground"
      role="status"
      data-testid="agent-memory-empty"
    >
      {{
        query.trim()
          ? t('settings.agentMemory.noMatches')
          : t('settings.agentMemory.empty')
      }}
    </p>

    <SettingsGroup v-if="pages.length > 0">
      <SettingsRow
        v-for="page in pages"
        :key="page.id"
        icon="lucide:file-text"
        :title="page.title"
        :description="describe(page)"
        :to="`/ai/memory/${page.id}`"
        data-testid="agent-memory-row"
        :data-page-id="page.id"
        :data-same-title="page.sameTitle || undefined"
      />
    </SettingsGroup>
  </section>
</template>
