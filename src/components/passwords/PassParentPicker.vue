<script setup lang="ts">
/**
 * Picks the entry a pass belongs to (spec 092, FR-013, FR-016): any login or pass, searched like
 * the list. The entries that probably fit (the same address host, or a title that holds the
 * pass's) come first; nothing is linked by itself, the user picks. The pass itself and every pass
 * below it are left out, so the pick cannot make a loop (the backend refuses one anyway).
 */
import { displayTitle } from '~/lib/passwords/format'
import { loopMembers, suggestParents } from '~/lib/passwords/links'
import { whenText } from '~/lib/passwords/passDate'
import { entryTypeOf } from '~/lib/passwords/passType'
import { filterHeaders } from '~/lib/passwords/search'
import { isInTrash, trashGroupIds } from '~/lib/passwords/tree'

const props = defineProps<{
  /** The pass being edited, left out of the list; `null` for a new one. */
  itemId: string | null
  /** What the pass is called and where it points, for the probable matches. */
  title: string
  url: string
}>()

const open = defineModel<boolean>('open', { required: true })
const emit = defineEmits<{ pick: [id: string] }>()

const { t, locale } = useI18n()
const fieldLabels = useFieldLabels()
const store = usePasswordsStore()

const query = ref('')
watch(open, (isOpen) => {
  if (isOpen) query.value = ''
})

/** The pass and everything that belongs to it, directly or further down. */
const below = computed(() => {
  const ids = new Set<string>()
  if (props.itemId === null) return ids
  ids.add(props.itemId)
  let grew = true
  while (grew) {
    grew = false
    for (const header of store.headers) {
      if (header.parentId && ids.has(header.parentId) && !ids.has(header.id)) {
        ids.add(header.id)
        grew = true
      }
    }
  }
  return ids
})

const candidates = computed(() => {
  const trash = trashGroupIds(store.groups)
  const onLoop = loopMembers(store.headers)
  const live = store.headers.filter(
    (header) =>
      !below.value.has(header.id) &&
      !onLoop.has(header.id) &&
      entryTypeOf(header) !== 'unknown' &&
      !isInTrash(header.groupId, trash),
  )
  const found = filterHeaders(live, { query: query.value })
  if (query.value.trim() !== '') return found.slice(0, 50)
  const { probable, others } = suggestParents(
    { title: props.title, url: props.url },
    found,
  )
  return [...probable, ...others].slice(0, 50)
})

const deviceZone = Intl.DateTimeFormat().resolvedOptions().timeZone

/** The line under a title: what tells entries of one name apart (a start or an expiry date), and
 * the note on a probable match. */
function describe(
  header: (typeof candidates.value)[number],
): string | undefined {
  const parts = [
    whenText(header, deviceZone, locale.value),
    probableIds.value.has(header.id)
      ? t('passwords.pass.links.probable')
      : null,
  ].filter((part): part is string => part !== null)
  return parts.length ? parts.join(' · ') : undefined
}

function pick(id: string) {
  emit('pick', id)
  open.value = false
}

const probableIds = computed(
  () =>
    new Set(
      suggestParents(
        { title: props.title, url: props.url },
        candidates.value,
      ).probable.map((header) => header.id),
    ),
)
</script>

<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('passwords.pass.links.pickTitle')"
  >
    <template #content>
      <div
        class="flex flex-col gap-3"
        data-testid="passwords-pass-parent-picker"
      >
        <UiInput
          v-model="query"
          type="search"
          :label="t('passwords.references.searchEntry')"
          :labels="fieldLabels.input.value"
          data-testid="passwords-pass-parent-search"
        />
        <p
          v-if="candidates.length === 0"
          class="py-4 text-center text-sm text-muted-foreground"
        >
          {{ t('passwords.noMatches') }}
        </p>
        <SettingsGroup v-else>
          <SettingsRow
            v-for="header in candidates"
            :key="header.id"
            :title="displayTitle(header.title) ?? t('passwords.untitled')"
            :description="describe(header)"
            navigates
            :data-testid="`passwords-pass-parent-option-${header.id}`"
            @click="pick(header.id)"
          />
        </SettingsGroup>
      </div>
    </template>
  </UiDrawerModal>
</template>
