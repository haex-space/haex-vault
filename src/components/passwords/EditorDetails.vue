<script setup lang="ts">
/**
 * The tab Details of the editor (spec 036, FR-001, FR-004; fields from spec 034 FR-001..FR-003):
 * title, username, password, address, TOTP (secret, digits, period, algorithm), note, expiry, tags,
 * icon and colour. The password is a plain field holding the stored value; every field copies and
 * the text fields take references from buttons in the field. An invalid TOTP value is refused at its field (`otpError`). The
 * generator opens in a drawer. The draft is the parent's object; this tab only edits it.
 */
import type { ItemDetail } from '@bindings/ItemDetail'
import type { Draft } from '~/lib/passwords/draft'
import { ENTRY_COLORS, ENTRY_ICONS } from '~/lib/passwords/icons'
import { clearOnReselect } from '~/lib/ui/radio'

const props = defineProps<{
  /** `null` is a new entry. */
  itemId: string | null
  detail: ItemDetail | null
  otpError: string | null
}>()
const draft = defineModel<Draft>({ required: true })

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { copyText } = usePasswordsCopy()

const tagInput = ref('')
const replacingOtp = ref(false)
const generatorOpen = ref(false)

const isNew = computed(() => props.itemId === null)

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

function startReplacingOtp() {
  replacingOtp.value = true
  draft.value.otp = {
    mode: 'set',
    text: '',
    digits: null,
    period: null,
    algorithm: null,
  }
}

function removeOtp() {
  replacingOtp.value = false
  draft.value.otp = { mode: 'clear' }
}

function otpText(): string {
  return draft.value.otp.mode === 'set' ? draft.value.otp.text : ''
}

/** The TOTP being set; a kept one becomes one being set with nothing filled in yet. */
function otpToSet() {
  if (draft.value.otp.mode !== 'set')
    draft.value.otp = {
      mode: 'set',
      text: '',
      digits: null,
      period: null,
      algorithm: null,
    }
  return draft.value.otp
}

function setOtpText(text: string) {
  otpToSet().text = text
}

/** An empty part is `null`: it comes from an otpauth address, else from the default. */
function setOtpNumber(part: 'digits' | 'period', value: unknown) {
  const number = Number.parseInt(String(value ?? ''), 10)
  otpToSet()[part] = Number.isNaN(number) ? null : number
}

function setOtpAlgorithm(value: string | null | undefined) {
  otpToSet().algorithm = value ?? null
}

const OTP_ALGORITHMS = ['SHA1', 'SHA256', 'SHA512'].map((name) => ({
  value: name,
  label: name,
}))

const showOtpInput = computed(
  () => isNew.value || !props.detail?.hasOtpSecret || replacingOtp.value,
)
</script>

<template>
  <div class="flex flex-col gap-5" data-testid="entry-editor-details">
    <SettingsGroup>
      <li class="px-4 py-3">
        <UiInput
          id="pw-title"
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
        <UiInput
          id="pw-username"
          v-model="draft.username"
          :label="t('passwords.fields.username')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          autocomplete="off"
          data-testid="passwords-field-username"
        >
          <template #append>
            <PasswordsCopyButton
              v-if="draft.username"
              :label="t('passwords.fields.username')"
              @copy="
                copyText(draft.username, t('passwords.fields.username'), {
                  itemId,
                  field: { kind: 'username' },
                })
              "
            />
            <PasswordsReferenceInsert
              v-model:text="draft.username"
              :item-id="itemId"
              kind="username"
            />
          </template>
        </UiInput>
        <PasswordsReferenceField
          v-model:text="draft.username"
          class="mt-1.5"
          kind="username"
        />
      </li>
      <li class="flex flex-col gap-1.5 px-4 py-3">
        <div class="flex items-center gap-2">
          <UiInputPassword
            id="pw-password"
            v-model="draft.password"
            :label="t('passwords.fields.password')"
            :labels="fieldLabels.password.value"
            label-bg="var(--muted)"
            autocomplete="new-password"
            class="flex-1"
            data-testid="passwords-field-password"
          >
            <template #append>
              <PasswordsCopyButton
                v-if="draft.password"
                :label="t('passwords.fields.password')"
                @copy="
                  copyText(draft.password, t('passwords.fields.password'), {
                    itemId,
                    field: { kind: 'password' },
                  })
                "
              />
              <PasswordsReferenceInsert
                v-model:text="draft.password"
                :item-id="itemId"
                kind="password"
              />
            </template>
          </UiInputPassword>
          <UiButtonIcon
            :label="t('passwords.generator.open')"
            variant="outline"
            data-testid="passwords-generate"
            @click="generatorOpen = true"
            ><Icon name="lucide:wand-sparkles" class="size-4"
          /></UiButtonIcon>
        </div>
        <PasswordsReferenceField
          v-model:text="draft.password"
          kind="password"
        />
      </li>
      <li class="px-4 py-3">
        <UiInput
          id="pw-url"
          v-model="draft.url"
          :label="t('passwords.fields.url')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          type="url"
          inputmode="url"
          data-testid="passwords-field-url"
        >
          <template #append>
            <PasswordsCopyButton
              v-if="draft.url"
              :label="t('passwords.fields.url')"
              @copy="
                copyText(draft.url, t('passwords.fields.url'), {
                  itemId,
                  field: { kind: 'url' },
                })
              "
            />
            <PasswordsReferenceInsert
              v-model:text="draft.url"
              :item-id="itemId"
              kind="url"
            />
          </template>
        </UiInput>
        <PasswordsReferenceField
          v-model:text="draft.url"
          class="mt-1.5"
          kind="url"
        />
      </li>
    </SettingsGroup>

    <SettingsGroup :label="t('passwords.fields.totp')">
      <li class="flex flex-col gap-1.5 px-4 py-3">
        <UiInput
          v-if="showOtpInput"
          id="pw-otp"
          :model-value="otpText()"
          :label="t('passwords.editor.otpLabel')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          :error="otpError ?? undefined"
          autocomplete="off"
          spellcheck="false"
          :placeholder="t('passwords.editor.otpPlaceholder')"
          data-testid="passwords-field-otp"
          @update:model-value="setOtpText(String($event ?? ''))"
        >
          <template v-if="otpText()" #append>
            <PasswordsCopyButton
              :label="t('passwords.history.fields.otpSecret')"
              @copy="
                copyText(otpText(), t('passwords.history.fields.otpSecret'))
              "
            />
          </template>
        </UiInput>
        <div v-if="showOtpInput" class="mt-1.5 grid grid-cols-3 gap-2">
          <UiInput
            id="pw-otp-digits"
            :model-value="draft.otp.mode === 'set' ? draft.otp.digits : null"
            :label="t('passwords.editor.otpDigits')"
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            type="number"
            min="6"
            max="10"
            placeholder="6"
            data-testid="passwords-field-otp-digits"
            @update:model-value="setOtpNumber('digits', $event)"
          />
          <UiInput
            id="pw-otp-period"
            :model-value="draft.otp.mode === 'set' ? draft.otp.period : null"
            :label="t('passwords.editor.otpPeriod')"
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            type="number"
            min="1"
            max="300"
            placeholder="30"
            data-testid="passwords-field-otp-period"
            @update:model-value="setOtpNumber('period', $event)"
          />
          <UiSelect
            id="pw-otp-algorithm"
            :model-value="draft.otp.mode === 'set' ? draft.otp.algorithm : null"
            :options="OTP_ALGORITHMS"
            :label="t('passwords.editor.otpAlgorithm')"
            label-bg="var(--muted)"
            data-testid="passwords-field-otp-algorithm"
            @update:model-value="setOtpAlgorithm"
          />
        </div>
        <div v-else class="flex flex-wrap items-center gap-2">
          <span
            class="min-w-0 flex-1 text-sm"
            :class="detail?.otpState === 'invalid' ? 'text-destructive' : ''"
          >
            {{
              detail?.otpState === 'invalid'
                ? t('passwords.totp.invalid')
                : t('passwords.editor.otpSet')
            }}
          </span>
          <UiButton
            type="button"
            variant="outline"
            size="sm"
            data-testid="passwords-replace-otp"
            @click="startReplacingOtp"
          >
            {{ t('passwords.editor.replace') }}
          </UiButton>
          <UiButton
            type="button"
            variant="outline"
            size="sm"
            data-testid="passwords-remove-otp"
            @click="removeOtp"
          >
            {{ t('passwords.totp.remove') }}
          </UiButton>
        </div>
        <p
          v-if="draft.otp.mode === 'clear'"
          class="text-sm text-muted-foreground"
        >
          {{ t('passwords.editor.otpWillBeRemoved') }}
        </p>
      </li>
    </SettingsGroup>

    <SettingsGroup>
      <li class="px-4 py-3">
        <UiTextarea
          id="pw-note"
          v-model="draft.note"
          :label="t('passwords.fields.note')"
          label-bg="var(--muted)"
          rows="4"
          data-testid="passwords-field-note"
        >
          <template #actions>
            <PasswordsCopyButton
              v-if="draft.note"
              :label="t('passwords.fields.note')"
              @copy="
                copyText(draft.note, t('passwords.fields.note'), {
                  itemId,
                  field: { kind: 'note' },
                })
              "
            />
            <PasswordsReferenceInsert
              v-model:text="draft.note"
              :item-id="itemId"
              kind="note"
            />
          </template>
        </UiTextarea>
        <PasswordsReferenceField
          v-model:text="draft.note"
          class="mt-1.5"
          kind="note"
        />
      </li>
      <li class="px-4 py-3">
        <div class="w-48">
          <UiInput
            id="pw-expires"
            v-model="draft.expiresAt"
            :label="t('passwords.fields.expires')"
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            type="date"
            data-testid="passwords-field-expires"
          >
            <template v-if="draft.expiresAt" #append>
              <PasswordsCopyButton
                :label="t('passwords.fields.expires')"
                @copy="copyText(draft.expiresAt, t('passwords.fields.expires'))"
              />
            </template>
          </UiInput>
        </div>
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
            <UiButtonClose
              class="size-5 rounded-full"
              icon-class="size-3"
              hover="destructive"
              :label="t('passwords.editor.removeTag', { tag })"
              size="icon-sm"
              @click="removeTag(tag)"
            />
          </ShadcnBadge>
        </div>
        <UiInput
          id="pw-tag"
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

    <UiDrawerModal
      v-model:open="generatorOpen"
      :title="t('passwords.generator.title')"
    >
      <template #content>
        <PasswordsGeneratorPanel
          embedded
          @use="
            (password: string) => {
              draft.password = password
              generatorOpen = false
            }
          "
        />
      </template>
    </UiDrawerModal>
  </div>
</template>
