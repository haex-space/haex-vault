<script setup lang="ts">
/**
 * The tab Details of the pass editor (spec 092, FR-002, FR-003, FR-012): title, kind, start (none, a
 * date alone, or a moment with the zone of its place), end, address, codes, note, tags, icon and
 * colour. The start is typed as the wall clock at the place and a zone; it becomes a point in time
 * when it is saved. The draft is the parent's object; this tab only edits it.
 */
import type { PassDraft } from '~/lib/passwords/passDraft'
import { isKnownZone } from '~/lib/passwords/passDraft'
import { ENTRY_COLORS, ENTRY_ICONS } from '~/lib/passwords/icons'
import { displayTitle } from '~/lib/passwords/format'
import { PASS_KINDS, passKindOf } from '~/lib/passwords/passType'
import { clearOnReselect } from '~/lib/ui/radio'

defineProps<{
  /** The pass being edited; `null` for a new one. */
  itemId?: string | null
  /** The reason the backend refused a code with, by index in the list. */
  refusedCodes?: Record<number, string>
  /** Whether the start cannot be saved as it is typed. */
  startInvalid?: boolean
}>()
const draft = defineModel<PassDraft>({ required: true })

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { copyText } = usePasswordsCopy()

const store = usePasswordsStore()
const tagInput = ref('')
const pickingParent = ref(false)
const deviceZone = Intl.DateTimeFormat().resolvedOptions().timeZone

const KIND_OPTIONS = PASS_KINDS.map((kind) => ({
  value: kind,
  label: t(`passwords.kinds.${kind}`),
}))
const START_OPTIONS = (['none', 'date', 'moment'] as const).map((mode) => ({
  value: mode,
  label: t(`passwords.pass.startModes.${mode}`),
}))

/** The kind shown: one this build does not know reads as "other" until the user picks one. */
const shownKind = computed(() => passKindOf({ passKind: draft.value.passKind }))

function setStartMode(mode: string | null | undefined) {
  if (mode === 'date') draft.value.start = { mode: 'date', date: '' }
  else if (mode === 'moment')
    draft.value.start = { mode: 'moment', wall: '', zone: deviceZone }
  else draft.value.start = { mode: 'none' }
}

const zoneUnknown = computed(
  () =>
    draft.value.start.mode === 'moment' &&
    draft.value.start.zone.trim() !== '' &&
    !isKnownZone(draft.value.start.zone.trim()),
)

/** The title of the entry this pass belongs to, as the headers hold it now. */
const parentTitle = computed(() => {
  if (!draft.value.parentId) return null
  const header = store.headersById.get(draft.value.parentId)
  return header
    ? (displayTitle(header.title) ?? t('passwords.untitled'))
    : t('passwords.pass.links.notAvailable')
})

function addTag() {
  const name = tagInput.value.trim()
  tagInput.value = ''
  if (!name) return
  const known = draft.value.tags.some(
    (tag) => tag.toLowerCase() === name.toLowerCase(),
  )
  if (!known) draft.value.tags = [...draft.value.tags, name]
}

function removeTag(name: string) {
  draft.value.tags = draft.value.tags.filter((tag) => tag !== name)
}
</script>

<template>
  <div class="flex flex-col gap-5" data-testid="pass-editor-details">
    <SettingsGroup>
      <li class="px-4 py-3">
        <UiInput
          id="pass-title"
          v-model="draft.title"
          :label="t('passwords.fields.title')"
          :placeholder="t('passwords.untitled')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          data-testid="passwords-field-title"
        >
          <template v-if="draft.title" #append>
            <PasswordsCopyButton
              :label="t('passwords.fields.title')"
              @copy="copyText(draft.title, t('passwords.fields.title'))"
            />
          </template>
        </UiInput>
      </li>
      <li class="px-4 py-3">
        <UiSelect
          id="pass-kind"
          :model-value="shownKind"
          :options="KIND_OPTIONS"
          :label="t('passwords.pass.kind')"
          label-bg="var(--muted)"
          data-testid="passwords-pass-field-kind"
          @update:model-value="draft.passKind = $event ?? 'other'"
        />
      </li>
    </SettingsGroup>

    <SettingsGroup :label="t('passwords.pass.links.belongsToTitle')">
      <li
        class="flex flex-wrap items-center gap-2 px-4 py-3"
        data-testid="passwords-pass-parent-field"
      >
        <span
          class="min-w-0 flex-1 truncate"
          :class="draft.parentId ? '' : 'text-muted-foreground'"
          data-testid="passwords-pass-parent-title"
        >
          {{ parentTitle ?? t('passwords.pass.links.none') }}
        </span>
        <UiButton
          type="button"
          variant="outline"
          size="sm"
          data-testid="passwords-pass-parent-choose"
          @click="pickingParent = true"
        >
          {{ t('passwords.pass.links.choose') }}
        </UiButton>
        <UiButton
          v-if="draft.parentId"
          type="button"
          variant="outline"
          size="sm"
          data-testid="passwords-pass-parent-clear"
          @click="draft.parentId = null"
        >
          {{ t('passwords.pass.links.clear') }}
        </UiButton>
      </li>
    </SettingsGroup>

    <SettingsGroup :label="t('passwords.pass.when')">
      <li class="flex flex-col gap-3 px-4 py-3">
        <UiSelect
          id="pass-start-mode"
          :model-value="draft.start.mode"
          :options="START_OPTIONS"
          :label="t('passwords.pass.start')"
          label-bg="var(--muted)"
          data-testid="passwords-pass-field-start-mode"
          @update:model-value="setStartMode"
        />
        <div v-if="draft.start.mode === 'date'" class="w-48">
          <UiInput
            id="pass-start-date"
            v-model="draft.start.date"
            :label="t('passwords.pass.startDate')"
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            type="date"
            data-testid="passwords-pass-field-start-date"
          />
        </div>
        <div
          v-else-if="draft.start.mode === 'moment'"
          class="flex flex-wrap items-start gap-3"
        >
          <div class="w-56">
            <UiInput
              id="pass-start-wall"
              v-model="draft.start.wall"
              :label="t('passwords.pass.startWall')"
              :labels="fieldLabels.input.value"
              label-bg="var(--muted)"
              type="datetime-local"
              data-testid="passwords-pass-field-start-wall"
            />
          </div>
          <div class="min-w-48 flex-1">
            <UiInput
              id="pass-start-zone"
              v-model="draft.start.zone"
              :label="t('passwords.pass.startZone')"
              :labels="fieldLabels.input.value"
              label-bg="var(--muted)"
              :placeholder="deviceZone"
              autocomplete="off"
              data-testid="passwords-pass-field-start-zone"
            />
          </div>
        </div>
        <p
          v-if="draft.start.mode === 'moment'"
          class="text-sm text-muted-foreground"
        >
          {{ t('passwords.pass.startZoneHint') }}
        </p>
        <p
          v-if="startInvalid || zoneUnknown"
          class="text-sm text-destructive"
          role="alert"
          data-testid="passwords-pass-start-error"
        >
          {{
            zoneUnknown
              ? t('passwords.pass.errors.zone')
              : t('passwords.pass.errors.start')
          }}
        </p>
      </li>
      <li class="px-4 py-3">
        <div class="w-48">
          <UiInput
            id="pass-expires"
            v-model="draft.expiresAt"
            :label="t('passwords.pass.expires')"
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            type="date"
            data-testid="passwords-field-expires"
          />
        </div>
      </li>
    </SettingsGroup>

    <SettingsGroup :label="t('passwords.pass.codes')">
      <li class="px-4 py-3" data-no-swipe>
        <PasswordsPassCodesEditor
          v-model="draft.codes"
          :refused="refusedCodes"
        />
      </li>
    </SettingsGroup>

    <SettingsGroup>
      <li class="px-4 py-3">
        <UiInput
          id="pass-url"
          v-model="draft.url"
          :label="t('passwords.fields.url')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          autocomplete="off"
          data-testid="passwords-field-url"
        />
      </li>
      <li class="px-4 py-3">
        <UiTextarea
          id="pass-note"
          v-model="draft.note"
          :label="t('passwords.fields.note')"
          label-bg="var(--muted)"
          rows="4"
          data-testid="passwords-field-note"
        />
      </li>
      <li class="flex flex-col gap-2 px-4 py-3">
        <div v-if="draft.tags.length" class="flex flex-wrap gap-1.5">
          <ShadcnBadge
            v-for="tag in draft.tags"
            :key="tag"
            variant="secondary"
            class="gap-1"
          >
            {{ tag }}
            <button
              type="button"
              class="rounded-full hover:text-destructive"
              :aria-label="t('passwords.editor.removeTag', { tag })"
              @click="removeTag(tag)"
            >
              <Icon name="lucide:x" class="size-3" />
            </button>
          </ShadcnBadge>
        </div>
        <UiInput
          id="pass-tag"
          v-model="tagInput"
          :label="t('passwords.fields.tags')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          :placeholder="t('passwords.editor.tagPlaceholder')"
          data-testid="passwords-field-tag"
          @keydown.enter.prevent="addTag"
          @blur="addTag"
        />
      </li>
    </SettingsGroup>

    <SettingsGroup :label="t('passwords.editor.look')">
      <li class="flex flex-col gap-3 px-4 py-3">
        <ShadcnRadioGroup
          v-model="draft.icon"
          class="flex flex-wrap gap-1.5"
          :aria-label="t('passwords.fields.icon')"
        >
          <UiRadioGroupTile
            v-for="name in ENTRY_ICONS"
            :key="name"
            :value="name"
            :aria-label="name.replace('lucide:', '')"
            class="flex size-9 items-center justify-center rounded-lg border border-transparent bg-background hover:bg-accent data-[state=checked]:border-primary data-[state=checked]:bg-primary/10"
            @select="
              clearOnReselect(
                $event,
                draft.icon === name,
                () => (draft.icon = null),
              )
            "
          >
            <Icon :name="name" class="size-5" />
          </UiRadioGroupTile>
        </ShadcnRadioGroup>
        <ShadcnRadioGroup
          v-model="draft.color"
          class="flex flex-wrap gap-1.5"
          :aria-label="t('passwords.fields.color')"
        >
          <UiRadioGroupTile
            v-for="color in ENTRY_COLORS"
            :key="color"
            :value="color"
            :aria-label="color"
            class="size-7 rounded-full border-2 border-transparent data-[state=checked]:border-foreground"
            :style="{ backgroundColor: color }"
            @select="
              clearOnReselect(
                $event,
                draft.color === color,
                () => (draft.color = null),
              )
            "
          />
        </ShadcnRadioGroup>
      </li>
    </SettingsGroup>

    <PasswordsPassParentPicker
      v-model:open="pickingParent"
      :item-id="itemId ?? null"
      :title="draft.title"
      :url="draft.url"
      @pick="draft.parentId = $event"
    />
  </div>
</template>
