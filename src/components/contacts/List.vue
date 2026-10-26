<script setup lang="ts">
/**
 * The list of the address book (spec 051, FR-002, FR-004): all contacts or those of a group,
 * alphabetical by name under letter headings, filtered by the search of the toolbar (`?q=`). A row
 * shows photo or initials, the name and the organization or first phone number, and a mark when a
 * change did not reach a phone (FR-025). With a phone of the vault syncing, contacts can be
 * selected and sent to it or taken off it together (FR-018c).
 */
import { filterContacts, sortByName } from '~/lib/contacts/search'
import { contactTitle, splitSelf } from '~/lib/contacts/self'
import { fold } from '~/lib/search/fold'

const { t } = useI18n()
const router = useTabRouter()
const store = useContactsStore()

const groupId = computed(() =>
  router.route.path.startsWith('/group/') ? router.route.params.id : null,
)
const group = computed(() =>
  store.groups.find((candidate) => candidate.id === groupId.value),
)

const shown = computed(() => {
  const inPlace = groupId.value
    ? store.headers.filter((header) => header.groups.includes(groupId.value!))
    : store.headers
  return sortByName(filterContacts(inPlace, router.route.query.q ?? ''))
})

// The own contact stands apart above the letters (spec 060 FR-001); a search or a group that does
// not match it leaves it out like any other contact.
const split = computed(() => splitSelf(shown.value))
const titleLabels = computed(() => ({
  self: t('contacts.self'),
  noName: t('contacts.noName'),
}))

/** The rows under their first letter; names that do not start with a letter go under "#". */
const sections = computed(() => {
  const result: { letter: string; headers: typeof shown.value }[] = []
  for (const header of split.value.others) {
    const first = [...fold(header.name)][0]?.toUpperCase() ?? ''
    const letter = /\p{L}/u.test(first) ? first : '#'
    const last = result.at(-1)
    if (last?.letter === letter) last.headers.push(header)
    else result.push({ letter, headers: [header] })
  }
  return result
})

const selecting = ref(false)
const selected = ref<ReadonlySet<string>>(new Set())

function toggle(id: string) {
  const next = new Set(selected.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  selected.value = next
}

function stopSelecting() {
  selecting.value = false
  selected.value = new Set()
}

function open(id: string, isSelf = false) {
  // The own contact goes to no phone, so it is never selected.
  if (selecting.value && !isSelf) toggle(id)
  else if (!selecting.value) void router.push(`/entry/${id}`)
}

function secondLine(header: (typeof shown.value)[number]): string {
  // The own contact reads "Ich"; its real name goes beneath.
  if (header.isSelf && header.name) return header.name
  return header.organization ?? header.phones[0] ?? header.emails[0] ?? ''
}
</script>

<template>
  <div class="flex min-h-0 flex-col gap-2 p-3" data-testid="contacts-list">
    <div class="flex items-center gap-2">
      <h1 class="min-w-0 flex-1 truncate px-1 text-2xl font-semibold">
        {{ group ? group.name : t('contacts.title') }}
      </h1>
      <UiButton
        v-if="store.phones.length && !selecting && split.others.length"
        variant="ghost"
        size="sm"
        data-testid="contacts-select"
        @click="selecting = true"
      >
        <Icon name="lucide:list-checks" class="size-4" />
        {{ t('contacts.selection.start') }}
      </UiButton>
    </div>
    <ContactsSelectionBar
      v-if="selecting"
      :ids="[...selected]"
      @done="stopSelecting"
    />
    <p
      v-if="store.isLoading && !store.hasLoadedOnce"
      class="p-4 text-muted-foreground"
    >
      {{ t('contacts.loading') }}
    </p>
    <p
      v-else-if="sections.length === 0 && !(split.own && router.route.query.q)"
      class="p-4 text-muted-foreground"
      data-testid="contacts-empty"
    >
      {{ router.route.query.q ? t('contacts.noMatch') : t('contacts.empty') }}
    </p>
    <section
      v-if="split.own"
      class="flex flex-col gap-1"
      data-testid="contacts-self"
    >
      <h2 class="px-2 pt-2 text-xs font-semibold text-muted-foreground">
        {{ t('contacts.self') }}
      </h2>
      <ul
        class="flex flex-col divide-y divide-background overflow-hidden rounded-xl bg-muted"
      >
        <li>
          <button
            type="button"
            class="flex min-h-14 w-full items-center gap-3 px-3 py-2 text-left hover:bg-foreground/5"
            :data-testid="`contacts-row-${split.own.id}`"
            @click="open(split.own.id, true)"
          >
            <ContactsAvatar :name="split.own.name" :photo="split.own.photo" />
            <span class="flex min-w-0 flex-1 flex-col">
              <span
                class="truncate font-medium"
                data-testid="contacts-row-name"
                >{{ contactTitle(split.own, titleLabels) }}</span
              >
              <span class="truncate text-sm text-muted-foreground">{{
                secondLine(split.own)
              }}</span>
            </span>
          </button>
        </li>
      </ul>
    </section>
    <section
      v-for="section in sections"
      :key="section.letter"
      class="flex flex-col gap-1"
    >
      <h2 class="px-2 pt-2 text-xs font-semibold text-muted-foreground">
        {{ section.letter }}
      </h2>
      <ul
        class="flex flex-col divide-y divide-background overflow-hidden rounded-xl bg-muted"
      >
        <li v-for="header in section.headers" :key="header.id">
          <button
            type="button"
            class="flex min-h-14 w-full items-center gap-3 px-3 py-2 text-left hover:bg-foreground/5"
            :aria-pressed="selecting ? selected.has(header.id) : undefined"
            :data-testid="`contacts-row-${header.id}`"
            @click="open(header.id)"
          >
            <ShadcnCheckbox
              v-if="selecting"
              :model-value="selected.has(header.id)"
              tabindex="-1"
              class="pointer-events-none"
              :data-testid="`contacts-row-check-${header.id}`"
            />
            <ContactsAvatar :name="header.name" :photo="header.photo" />
            <span class="flex min-w-0 flex-1 flex-col">
              <span
                class="truncate font-medium"
                data-testid="contacts-row-name"
                >{{ header.name || t('contacts.noName') }}</span
              >
              <span class="truncate text-sm text-muted-foreground">{{
                secondLine(header)
              }}</span>
            </span>
            <Icon
              v-if="header.issue"
              name="lucide:circle-alert"
              class="size-4 shrink-0 text-destructive"
              :aria-label="t(`settings.contacts.phone.issues.${header.issue}`)"
              :data-testid="`contacts-row-issue-${header.id}`"
            />
            <Icon
              v-else-if="header.targets.length"
              name="lucide:smartphone"
              class="size-4 shrink-0 text-muted-foreground"
              :aria-label="t('contacts.targets.onPhone')"
              :data-testid="`contacts-row-on-phone-${header.id}`"
            />
          </button>
        </li>
      </ul>
    </section>
  </div>
</template>
