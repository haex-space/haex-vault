<script setup lang="ts">
/**
 * A searchable dropdown over the tags of the vault (spec 100, FR-001, FR-002): the trigger shows the
 * chosen tag, the popover holds a search field and the tags with their color and how often each is
 * used per app. Typing narrows the list by `fold` (case, accents and the form of umlauts do not
 * matter, like every search in the app). The item value is the tag's id, so equal names never
 * collide. `exclude` leaves one tag out (the source of a merge); attributes such as `data-testid`
 * go to the trigger.
 */
import {
  ListboxContent,
  ListboxFilter,
  ListboxItem,
  ListboxRoot,
} from 'reka-ui'
import type { MergedTag } from '~/lib/passwords/remote'
import { tagUsageOf } from '~/lib/passwords/scope'
import { fold } from '~/lib/search/fold'

defineOptions({ inheritAttrs: false })

const props = defineProps<{
  tags: readonly MergedTag[]
  exclude?: string
  placeholder?: string
  /** Shows how often the chosen tag is used per app inside the trigger. */
  showUsage?: boolean
}>()
const model = defineModel<string | null>({ required: true })

const { t } = useI18n()
const store = usePasswordsStore()

const open = ref(false)
const query = ref('')
// The search belongs to one opening: a closed picker starts from the whole list again.
watch(open, (isOpen) => {
  if (!isOpen) query.value = ''
})

const candidates = computed(() =>
  props.tags.filter((tag) => tag.id !== props.exclude),
)
// ponytail: one pass per tag over the headers, computed once per change; fine for a few hundred
// tags, upgrade path is one pass over the headers that fills a map of counters.
const usageById = computed(
  () =>
    new Map(
      candidates.value.map((tag) => [
        tag.id,
        tagUsageOf(tag.ids, store.headers),
      ]),
    ),
)
const shown = computed(() => {
  const wanted = fold(query.value.trim())
  return wanted
    ? candidates.value.filter((tag) => fold(tag.name).includes(wanted))
    : candidates.value
})
const chosen = computed(
  () => props.tags.find((tag) => tag.id === model.value) ?? null,
)

function pick(value: unknown) {
  model.value = String(value)
  open.value = false
}
</script>

<template>
  <ShadcnPopover v-model:open="open">
    <ShadcnPopoverTrigger as-child>
      <button
        v-bind="$attrs"
        type="button"
        role="combobox"
        aria-haspopup="listbox"
        :aria-expanded="open"
        class="flex h-9 w-full items-center gap-2 rounded-md border border-input bg-transparent px-3 text-left text-sm shadow-xs focus-visible:outline-none focus-visible:ring-[3px] focus-visible:ring-primary/50"
      >
        <template v-if="chosen">
          <span
            class="size-2.5 shrink-0 rounded-full border border-foreground/20"
            :style="
              chosen.color ? { backgroundColor: chosen.color } : undefined
            "
            aria-hidden="true"
          />
          <span class="min-w-0 flex-1 truncate">{{ chosen.name }}</span>
        </template>
        <span v-else class="min-w-0 flex-1 truncate text-muted-foreground">{{
          placeholder ?? t('passwords.tags.choose')
        }}</span>
        <PasswordsTagUsage
          v-if="showUsage && chosen && usageById.has(chosen.id)"
          :usage="usageById.get(chosen.id)!"
        />
        <Icon
          name="lucide:chevrons-up-down"
          class="size-4 shrink-0 opacity-50"
          aria-hidden="true"
        />
      </button>
    </ShadcnPopoverTrigger>
    <ShadcnPopoverContent
      align="start"
      class="w-80 max-w-[calc(100vw-2rem)] p-0"
    >
      <ListboxRoot :model-value="model ?? undefined" @update:model-value="pick">
        <div class="flex items-center border-b px-3">
          <Icon
            name="lucide:search"
            class="mr-2 size-4 shrink-0 opacity-50"
            aria-hidden="true"
          />
          <ListboxFilter
            v-model="query"
            auto-focus
            :placeholder="t('passwords.tags.search')"
            :aria-label="t('passwords.tags.search')"
            class="flex h-10 w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
            data-testid="passwords-tag-picker-search"
          />
        </div>
        <ListboxContent class="max-h-72 overflow-y-auto overflow-x-hidden p-1">
          <ListboxItem
            v-for="tag in shown"
            :key="tag.id"
            :value="tag.id"
            class="flex cursor-default select-none items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-none data-[highlighted]:bg-accent data-[highlighted]:text-accent-foreground"
            :data-testid="`passwords-tag-picker-${tag.id}`"
          >
            <span
              class="size-2.5 shrink-0 rounded-full border border-foreground/20"
              :style="tag.color ? { backgroundColor: tag.color } : undefined"
              aria-hidden="true"
            />
            <span class="min-w-0 flex-1 truncate">{{ tag.name }}</span>
            <PasswordsTagUsage :usage="usageById.get(tag.id)!" />
          </ListboxItem>
          <p
            v-if="shown.length === 0"
            class="px-2 py-6 text-center text-sm text-muted-foreground"
          >
            {{ t('passwords.tags.noMatch') }}
          </p>
        </ListboxContent>
      </ListboxRoot>
    </ShadcnPopoverContent>
  </ShadcnPopover>
</template>
