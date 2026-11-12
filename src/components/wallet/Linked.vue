<script setup lang="ts">
/**
 * The "Linked" section of an entry (spec 092, FR-014): the passes that belong directly to it, with
 * title, start or end date and whether each is still valid, in the order of FR-021 (upcoming first,
 * then over, then undated). It is a query over the links of the headers the window holds and has no
 * storage of its own; a pass in the trash is not listed. Nothing shows for an entry without passes.
 */
import { displayTitle, isExpired, localDay } from '~/lib/passwords/format'
import { linkedOf } from '~/lib/passwords/links'
import { formatStart } from '~/lib/passwords/passDate'
import { PASS_KIND_ICONS, passKindOf } from '~/lib/passwords/passType'
import { trashGroupIds } from '~/lib/passwords/tree'

const props = defineProps<{ itemId: string }>()

const { t, locale } = useI18n()
const openEntry = useOpenEntry()
const store = usePasswordsStore()

const deviceZone = Intl.DateTimeFormat().resolvedOptions().timeZone
const passes = computed(() =>
  linkedOf(
    props.itemId,
    store.headers,
    new Date(),
    trashGroupIds(store.groups),
  ),
)

function whenOf(header: (typeof passes.value)[number]): string | null {
  const start = formatStart(header, deviceZone, locale.value)
  return start?.place ?? header.expiresAt ?? null
}

function expired(header: (typeof passes.value)[number]): boolean {
  return isExpired(header.expiresAt, localDay(new Date()))
}
</script>

<template>
  <SettingsGroup
    v-if="passes.length"
    :label="t('passwords.pass.links.linked')"
    data-testid="wallet-linked"
  >
    <li v-for="header in passes" :key="header.id">
      <button
        type="button"
        class="flex w-full items-center gap-3 px-4 py-3 text-left hover:bg-accent"
        :data-testid="`wallet-linked-${header.id}`"
        @click="openEntry(header)"
      >
        <Icon
          :name="PASS_KIND_ICONS[passKindOf(header)]"
          class="size-5 shrink-0"
        />
        <span class="flex min-w-0 flex-1 flex-col">
          <span class="truncate">{{
            displayTitle(header.title) ?? t('passwords.untitled')
          }}</span>
          <span
            v-if="whenOf(header)"
            class="truncate text-sm text-muted-foreground"
            >{{ whenOf(header) }}</span
          >
        </span>
        <ShadcnBadge :variant="expired(header) ? 'destructive' : 'secondary'">
          {{
            expired(header)
              ? t('passwords.pass.links.expired')
              : t('passwords.pass.links.valid')
          }}
        </ShadcnBadge>
      </button>
    </li>
  </SettingsGroup>
</template>
