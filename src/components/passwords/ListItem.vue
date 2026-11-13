<script setup lang="ts">
/**
 * One entry in the list (spec 034, FR-001, FR-008, US1): icon, title (or the placeholder text of the
 * window for an entry without one), username, tag chips, a badge when it has expired and markers
 * for a TOTP and for passkeys, and one for an entry that belongs to a holzi function (spec 038, Z14). It holds a header only, never a secret. Spec 036 adds the context
 * menu and the menu button (FR-018), the dimming of a cut entry (FR-013) and the one tab stop of
 * the list (arrow keys move it, FR-016).
 */
import type { ItemHeader } from '@bindings/ItemHeader'
import type { MenuCommand, MenuEntry } from '~/lib/passwords/menus'
import { DEFAULT_ENTRY_ICON } from '~/lib/passwords/icons'
import { draggedIds, ITEMS_MIME, itemsPayload } from '~/lib/passwords/dnd'
import { displayTitle, isExpired, localDay } from '~/lib/passwords/format'
import { placeholderParts } from '~/lib/passwords/search'
import { entryTypeOf } from '~/lib/passwords/passType'

const props = defineProps<{
  header: ItemHeader
  /** Whether the entry is part of the selection. */
  selected?: boolean
  /** Whether a selection is going on (a plain click then toggles instead of opening). */
  selecting?: boolean
  /** The entry lies cut in the Ablage. */
  dimmed?: boolean
  /** The row holds the one tab stop of the list. */
  tabStop?: boolean
  /** Builds the menu of a row when it opens; the same function for every row. */
  menuFor: (id: string) => readonly MenuEntry[]
  /** Position and size of the whole list for assistive technology: only a page of the rows is
   * mounted, so the list itself would announce too few. */
  position?: number
  count?: number
}>()

const emit = defineEmits<{
  /** A click with its modifier keys; the list decides between open, toggle and range. */
  activate: [event: MouseEvent]
  /** A long press on a touch screen starts or extends the selection. */
  longPress: []
  menu: [command: MenuCommand]
  /** A menu of the row opens. */
  menuOpen: []
  focus: []
}>()

const { t } = useI18n()

const title = computed(() => displayTitle(props.header.title))
const expired = computed(() =>
  isExpired(props.header.expiresAt, localDay(new Date())),
)

/** An entry of a type a newer holzi wrote: shown with a notice, never as an empty login. */
const unknownType = computed(() => entryTypeOf(props.header) === 'unknown')

const selection = usePasswordsSelectionStore()

const button = useTemplateRef<HTMLElement>('button')
const press = usePasswordsRowPress(button, {
  activate: (event) => emit('activate', event),
  longPress: () => emit('longPress'),
})

function onDragStart(event: DragEvent) {
  const ids = draggedIds(props.header.id, selection.ids)
  event.dataTransfer?.setData(ITEMS_MIME, itemsPayload(ids))
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move'
}
</script>

<template>
  <PasswordsEntryMenu
    :entries="() => menuFor(header.id)"
    @run="emit('menu', $event)"
    @open="emit('menuOpen')"
  >
    <li
      class="group flex items-center pr-2 hover:bg-foreground/5"
      :aria-posinset="position"
      :aria-setsize="count"
      :class="[
        selected ? 'bg-primary/10' : '',
        dimmed ? 'opacity-50 grayscale' : '',
      ]"
      @contextmenu.stop
    >
      <button
        ref="button"
        type="button"
        class="flex min-h-14 min-w-0 flex-1 items-center gap-4 py-3 pl-4 text-left focus-visible:outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:ring-inset"
        :aria-pressed="selecting ? selected : undefined"
        :tabindex="tabStop ? 0 : -1"
        draggable="true"
        :data-row-id="header.id"
        :data-testid="`passwords-entry-${header.id}`"
        @click="press.onClick"
        @contextmenu="press.onContextmenu"
        @focus="emit('focus')"
        @dragstart="onDragStart"
        @pointerdown="press.onPointerdown"
      >
        <ShadcnCheckbox
          v-if="selecting"
          :model-value="selected"
          class="pointer-events-none shrink-0"
          tabindex="-1"
          aria-hidden="true"
        />
        <span
          class="flex size-9 shrink-0 items-center justify-center rounded-lg bg-background"
          :style="header.color ? { color: header.color } : undefined"
          aria-hidden="true"
        >
          <span class="size-5">
            <PasswordsEntryIcon
              :value="header.icon"
              :fallback="DEFAULT_ENTRY_ICON"
            />
          </span>
        </span>
        <span class="flex min-w-0 flex-1 flex-col">
          <span
            class="truncate"
            :class="title === null ? 'text-muted-foreground italic' : ''"
          >
            {{ title ?? t('passwords.untitled') }}
          </span>
          <span
            v-if="header.username"
            class="truncate text-sm text-muted-foreground"
            data-testid="passwords-list-username"
          >
            <template
              v-for="(part, index) in placeholderParts(header.username)"
              :key="index"
            >
              <template v-if="part.kind === 'text'">{{ part.text }}</template>
              <span
                v-else
                class="mx-0.5 inline-flex items-center gap-0.5 rounded-full border border-primary/40 bg-primary/10 px-1.5 align-middle text-xs"
                data-testid="passwords-list-reference"
              >
                <Icon name="lucide:link" class="size-3" />
                {{ t('passwords.references.listMark') }}
              </span>
            </template>
          </span>
        </span>
        <span class="flex shrink-0 flex-wrap items-center justify-end gap-1.5">
          <ShadcnBadge
            v-if="unknownType"
            variant="outline"
            data-testid="passwords-unknown-type-badge"
          >
            {{ t('passwords.unknownType.badge') }}
          </ShadcnBadge>
          <ShadcnBadge v-if="expired" variant="destructive">
            {{ t('passwords.expired') }}
          </ShadcnBadge>
          <ShadcnBadge
            v-if="header.owner"
            variant="secondary"
            data-testid="passwords-owner-badge"
          >
            {{
              t('passwords.owner', {
                name: t(`passwords.owners.${header.owner}`),
              })
            }}
          </ShadcnBadge>
          <span
            v-if="header.hasTotp"
            class="text-muted-foreground"
            :title="t('passwords.markers.totp')"
          >
            <Icon name="lucide:timer" class="size-4" />
            <span class="sr-only">{{ t('passwords.markers.totp') }}</span>
          </span>
          <span
            v-if="header.passkeyCount > 0"
            class="text-muted-foreground"
            :title="t('passwords.markers.passkey')"
          >
            <Icon name="lucide:fingerprint" class="size-4" />
            <span class="sr-only">{{ t('passwords.markers.passkey') }}</span>
          </span>
          <ShadcnBadge
            v-for="tag in header.tags"
            :key="tag.id"
            variant="secondary"
            :style="tag.color ? { color: tag.color } : undefined"
          >
            {{ tag.name }}
          </ShadcnBadge>
        </span>
      </button>
      <PasswordsEntryMenuButton
        :entries="() => menuFor(header.id)"
        :label="
          t('passwords.menu.button', { name: title ?? t('passwords.untitled') })
        "
        :data-testid="`passwords-entry-menu-${header.id}`"
        @run="emit('menu', $event)"
        @open="emit('menuOpen')"
      />
    </li>
  </PasswordsEntryMenu>
</template>
