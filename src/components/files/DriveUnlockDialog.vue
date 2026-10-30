<script setup lang="ts">
/**
 * The password of a locked drive (spec 064 US3, US4, contracts/ui.md): typed, or an entry picked
 * from the password manager, and remembered there on request once the drive is open (FR-014 to
 * FR-016). A wrong password keeps the dialog open with the system's text (FR-015); the field is
 * emptied after every attempt and on close (FR-017). Without an open vault the password manager
 * part is greyed out (FR-018).
 */
import { toast } from 'vue-sonner'
import type { Drive } from '@bindings/Drive'
import type { DriveEntry } from '@bindings/DriveEntry'
import type { DriveNeedsPassword } from '@bindings/DriveNeedsPassword'
import { asFilesError } from '~/composables/useFiles'

const props = defineProps<{
  drive: Drive | null
  reason: DriveNeedsPassword
  message: string | null
  /** The saved entries that fit, for `several`. */
  matches: DriveEntry[]
}>()
const emit = defineEmits<{ opened: [path: string] }>()
const open = defineModel<boolean>('open', { required: true })

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { driveUnlockCandidatesAsync, driveLinkEntryAsync } = useFiles()
const { nameOf, unlockDriveAsync } = useFilesDriveActions()

const password = ref('')
const entryId = ref('')
const remember = ref(false)
const error = ref<string | null>(null)
const unlocking = ref(false)
const picking = ref(false)
const search = ref('')
const entries = ref<DriveEntry[] | null>(null)
const noVault = ref(false)
/** After an unlock whose entry has another address: the question, then the path to open. */
const asking = ref<{ entry: DriveEntry; path: string } | null>(null)

const name = computed(() => (props.drive ? nameOf(props.drive) : ''))
const hint = computed(() => {
  if (props.reason === 'several') return t('files.drives.unlock.several')
  if (props.reason !== 'savedFailed') return null
  return props.message
    ? t('files.drives.unlock.savedFailed', { message: props.message })
    : t('files.drives.unlock.savedFailedGeneric')
})
const shown = computed(() => {
  const query = search.value.trim().toLowerCase()
  const all = entries.value ?? []
  if (!query) return all
  return all.filter((entry) =>
    [entry.title, entry.username, entry.url].some((text) =>
      text?.toLowerCase().includes(query),
    ),
  )
})

watch(open, (isOpen) => {
  password.value = ''
  error.value = null
  asking.value = null
  if (!isOpen) return
  entryId.value = ''
  remember.value = false
  search.value = ''
  noVault.value = false
  picking.value = props.reason === 'several'
  entries.value = props.reason === 'several' ? props.matches : null
  // Loaded at once, so a closed vault greys out the checkbox before the first click.
  if (props.reason !== 'several') void loadAsync()
})

async function loadAsync() {
  try {
    entries.value = await driveUnlockCandidatesAsync('')
  } catch (cause) {
    entries.value = []
    if (asFilesError(cause)?.code === 'noVault') noVault.value = true
    else error.value = asFilesError(cause)?.message ?? String(cause)
  }
}

// One way at a time: typing drops a picked entry and picking empties the field.
watch(password, (typed) => {
  if (typed) entryId.value = ''
})
watch(entryId, (picked) => {
  if (picked) password.value = ''
})

const ready = computed(() => password.value !== '' || entryId.value !== '')

function describe(entry: DriveEntry) {
  return [entry.username, entry.url].filter(Boolean).join(' · ')
}

function done(path: string) {
  open.value = false
  emit('opened', path)
}

async function unlockAsync() {
  const drive = props.drive
  if (!drive || !ready.value) return
  const picked = entries.value?.find((entry) => entry.itemId === entryId.value)
  const secret = picked
    ? { kind: 'entry' as const, itemId: picked.itemId }
    : { kind: 'typed' as const, password: password.value }
  password.value = ''
  error.value = null
  unlocking.value = true
  try {
    const result = await unlockDriveAsync(
      drive,
      secret,
      remember.value && !noVault.value ? 'yes' : 'no',
    )
    if (result === null) {
      open.value = false
      return
    }
    if (result.kind === 'unlockFailed') {
      error.value = result.message
      return
    }
    if (result.rememberFailed === 'urlTaken' && picked) {
      asking.value = { entry: picked, path: result.path }
      return
    }
    if (result.rememberFailed === 'failed')
      toast.error(t('files.drives.unlock.rememberFailed', { name: name.value }))
    done(result.path)
  } finally {
    unlocking.value = false
  }
}

async function answerAsync(replace: boolean) {
  const pending = asking.value
  if (!pending) return
  if (replace && props.drive?.key) {
    try {
      await driveLinkEntryAsync(props.drive.key, pending.entry.itemId)
    } catch {
      toast.error(t('files.drives.unlock.rememberFailed', { name: name.value }))
    }
  }
  done(pending.path)
}
</script>

<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('files.drives.unlock.title', { name })"
  >
    <template #content>
      <div
        v-if="asking"
        class="flex flex-col gap-4"
        data-testid="files-drive-unlock-url-taken"
      >
        <p class="text-sm">
          {{
            t('files.drives.unlock.urlTaken', {
              url: asking.entry.url ?? '',
            })
          }}
        </p>
        <div class="flex justify-end gap-2">
          <UiButton
            type="button"
            variant="outline"
            data-testid="files-drive-unlock-keep"
            @click="answerAsync(false)"
            >{{ t('files.drives.unlock.keep') }}</UiButton
          >
          <UiButton
            type="button"
            data-testid="files-drive-unlock-replace"
            @click="answerAsync(true)"
            >{{ t('files.drives.unlock.replace') }}</UiButton
          >
        </div>
      </div>
      <form
        v-else
        class="flex flex-col gap-4"
        data-testid="files-drive-unlock-form"
        @submit.prevent="unlockAsync"
      >
        <p
          v-if="hint"
          class="text-sm text-muted-foreground"
          role="status"
          data-testid="files-drive-unlock-hint"
        >
          {{ hint }}
        </p>
        <UiInputPassword
          id="files-drive-unlock-password"
          v-model="password"
          :label="t('files.drives.unlock.password')"
          :labels="fieldLabels.password.value"
          autocomplete="off"
          data-testid="files-drive-unlock-password"
        />
        <p
          v-if="error"
          class="text-sm text-destructive"
          role="alert"
          data-testid="files-drive-unlock-error"
        >
          {{ error }}
        </p>

        <UiButton
          v-if="!picking"
          type="button"
          variant="outline"
          data-testid="files-drive-unlock-pick"
          @click="picking = true"
          >{{ t('files.drives.unlock.pick') }}</UiButton
        >
        <section
          v-else
          class="flex flex-col gap-2"
          :class="noVault && 'opacity-50'"
          :aria-disabled="noVault"
        >
          <h2 class="px-1 text-sm font-semibold">
            {{ t('files.drives.unlock.pick') }}
          </h2>
          <p
            v-if="noVault"
            class="px-1 text-sm text-muted-foreground"
            role="status"
            data-testid="files-drive-unlock-no-vault"
          >
            {{ t('files.drives.unlock.noVault') }}
          </p>
          <template v-else>
            <UiInput
              v-if="reason !== 'several'"
              id="files-drive-unlock-search"
              v-model="search"
              :label="t('files.drives.unlock.search')"
              :labels="fieldLabels.input.value"
              type="search"
              autocomplete="off"
              data-testid="files-drive-unlock-search"
            />
            <p
              v-if="entries === null"
              class="px-1 text-sm text-muted-foreground"
              role="status"
            >
              {{ t('files.drives.unlock.loading') }}
            </p>
            <p
              v-else-if="entries.length === 0"
              class="px-1 text-sm text-muted-foreground"
              role="status"
            >
              {{ t('files.drives.unlock.empty') }}
            </p>
            <p
              v-else-if="shown.length === 0"
              class="px-1 text-sm text-muted-foreground"
              role="status"
            >
              {{ t('files.drives.unlock.noMatch') }}
            </p>
            <div v-else class="max-h-64 overflow-y-auto rounded-xl">
              <SettingsGroup v-model="entryId" radio>
                <SettingsOptionRow
                  v-for="entry in shown"
                  :key="entry.itemId"
                  type="radio"
                  :value="entry.itemId"
                  :title="entry.title || entry.username || entry.itemId"
                  :description="describe(entry)"
                  :data-testid="`files-drive-unlock-entry-${entry.itemId}`"
                />
              </SettingsGroup>
            </div>
          </template>
        </section>

        <label
          class="flex items-center gap-3 text-sm"
          :class="noVault && 'opacity-50'"
        >
          <ShadcnCheckbox
            v-model="remember"
            :disabled="noVault"
            data-testid="files-drive-unlock-remember"
          />
          {{
            entryId
              ? t('files.drives.unlock.rememberEntry')
              : t('files.drives.unlock.rememberTyped')
          }}
        </label>

        <div class="flex justify-end gap-2">
          <UiButton type="button" variant="outline" @click="open = false">{{
            t('files.cancel')
          }}</UiButton>
          <UiButton
            type="submit"
            :disabled="!ready"
            :loading="unlocking"
            data-testid="files-drive-unlock-submit"
            >{{ t('files.drives.unlock.submit') }}</UiButton
          >
        </div>
      </form>
    </template>
  </UiDrawerModal>
</template>
