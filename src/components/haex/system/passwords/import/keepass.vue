<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
    :description="t('selectFile')"
  >
    <template #body>
      <div class="space-y-4">
        <div class="space-y-2">
          <p class="text-sm font-medium">
            {{ t('kdbxFile') }}
          </p>
          <input
            ref="fileInput"
            type="file"
            accept=".kdbx"
            class="hidden"
            @change="onFileChangeAsync"
          />
          <UButton
            icon="i-lucide-file"
            variant="outline"
            color="neutral"
            class="w-full justify-start"
            @click="fileInput?.click()"
          >
            {{ selectedFileName || t('chooseFile') }}
          </UButton>
        </div>

        <div
          v-if="fileData"
          class="space-y-2"
        >
          <p class="text-sm font-medium">
            {{ t('password') }}
          </p>
          <div class="flex gap-1">
            <UInput
              v-model="password"
              :type="showPassword ? 'text' : 'password'"
              :placeholder="t('passwordPlaceholder')"
              class="flex-1"
              @keyup.enter="canImport && importAsync()"
            />
            <UiButton
              :icon="showPassword ? 'i-lucide-eye-off' : 'i-lucide-eye'"
              color="neutral"
              variant="outline"
              type="button"
              @click="showPassword = !showPassword"
            />
          </div>
        </div>

        <div
          v-if="importing"
          class="space-y-2"
        >
          <UProgress :value="progress" />
          <p class="text-sm text-center text-muted">
            {{ t('importing') }}: {{ progress }}%
          </p>
        </div>

        <div
          v-if="error"
          class="p-4 bg-error/10 text-error rounded-lg text-sm"
        >
          {{ error }}
        </div>
      </div>
    </template>

    <template #footer>
      <div class="flex gap-2 justify-end">
        <UButton
          color="neutral"
          variant="outline"
          @click="open = false"
        >
          {{ t('cancel') }}
        </UButton>
        <UButton
          :disabled="!canImport"
          :loading="importing"
          @click="importAsync"
        >
          {{ t('import') }}
        </UButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import * as kdbxweb from 'kdbxweb'
import { argon2id, argon2i, argon2d } from 'hash-wasm'
import { useToast } from '#imports'
import {
  haexPasswordsItemDetails,
  haexPasswordsGroupItems,
  haexPasswordsItemKeyValues,
  haexPasswordsItemBinaries,
  haexPasswordsItemSnapshots,
  haexPasswordsSnapshotBinaries,
} from '~/database/schemas'
import { requireDb } from '~/stores/vault'
import { addBinaryAsync } from '~/utils/passwords/binaries'
import type { SnapshotData } from '~/utils/passwords/snapshots'

// Plug in hash-wasm Argon2 for KeePass 4 databases.
// Type: 0 = Argon2d, 1 = Argon2i, 2 = Argon2id
kdbxweb.CryptoEngine.argon2 = async (
  password: ArrayBuffer,
  salt: ArrayBuffer,
  memory: number,
  iterations: number,
  length: number,
  parallelism: number,
  type: number,
) => {
  const params = {
    password: new Uint8Array(password),
    salt: new Uint8Array(salt),
    parallelism, iterations, memorySize: memory, hashLength: length,
    outputType: 'binary' as const,
  }
  let result: Uint8Array
  if (type === 0) result = await argon2d(params)
  else if (type === 1) result = await argon2i(params)
  else result = await argon2id(params)

  const buf = new ArrayBuffer(result.byteLength)
  new Uint8Array(buf).set(result)
  return buf
}

const open = defineModel<boolean>('open', { default: false })
const { t } = useI18n()
const toast = useToast()

const fileInput = useTemplateRef<HTMLInputElement>('fileInput')
const fileData = ref<ArrayBuffer | null>(null)
const selectedFileName = ref<string | null>(null)
const password = ref('')
const showPassword = ref(false)
const importing = ref(false)
const progress = ref(0)
const error = ref<string | null>(null)

const canImport = computed(() => !!fileData.value && !!password.value && !importing.value)

const onFileChangeAsync = async (event: Event) => {
  const target = event.target as HTMLInputElement
  const file = target.files?.[0]
  if (!file) { selectedFileName.value = null; return }
  selectedFileName.value = file.name
  error.value = null; password.value = ''
  try {
    fileData.value = await file.arrayBuffer()
  } catch (err) {
    error.value = t('error.parse'); console.error(err)
  }
}

const importAsync = async () => {
  if (!fileData.value) { error.value = t('error.noFile'); return }
  if (!password.value) { error.value = t('error.noPassword'); return }

  importing.value = true; progress.value = 0; error.value = null
  try {
    const stats = await importKdbxAsync(fileData.value, password.value)
    toast.add({
      title: t('success'),
      description: t('successDescription', { groups: stats.groupCount, entries: stats.entryCount }),
      color: 'success',
    })
    open.value = false
    fileData.value = null; password.value = ''; selectedFileName.value = null
  } catch (err) {
    console.error('[KeePass Import]', err)
    const msg = err instanceof Error ? err.message : String(err)
    error.value = msg.includes('InvalidKey') || msg.includes('password')
      ? t('error.wrongPassword')
      : t('error.import') + ': ' + msg
  } finally {
    importing.value = false; progress.value = 0
  }
}

// Maps KeePass standard icon IDs (0–68) to Iconify names.
// KeePass stores these as integers in entry.icon / group.icon;
// they are NOT embedded in the KDBX file — this mapping is the iconset.
// Primary: i-lucide-* (already used throughout the app, all verified to exist).
// MDI fallback only for brand icons (Firefox, Chrome, Windows, etc.) with no Lucide equivalent.
const KEEPASS_ICONS: string[] = [
  'i-lucide-key',            // 0  Key
  'i-lucide-globe',          // 1  World / Network
  'i-lucide-triangle-alert', // 2  Warning
  'i-lucide-server',         // 3  Network Server
  'i-lucide-folder-check',   // 4  Marked Directory
  'i-lucide-message-circle', // 5  User Communication
  'i-lucide-puzzle',         // 6  Parts
  'i-lucide-notebook',       // 7  Notepad
  'i-lucide-network',        // 8  World Socket / Network
  'i-lucide-contact',        // 9  Identity
  'i-lucide-file-check',     // 10 Paper Ready
  'i-lucide-camera',         // 11 Digicam
  'i-lucide-bluetooth',      // 12 IR Communication
  'i-lucide-key-round',      // 13 Multi Keys
  'i-lucide-zap',            // 14 Energy
  'i-lucide-scan',           // 15 Scanner
  'i-lucide-wifi',           // 16 World Star / Broadcast
  'i-lucide-mailbox',        // 17 Envelope Box
  'i-lucide-hard-drive',     // 18 Disk
  'i-lucide-monitor',        // 19 Monitor
  'i-lucide-mail',           // 20 EMail
  'i-lucide-settings',       // 21 Configuration
  'i-lucide-clipboard',      // 22 Clipboard Ready
  'i-lucide-file-plus',      // 23 Paper New
  'i-lucide-terminal',       // 24 Screen / Terminal
  'i-lucide-plug',           // 25 Energy Careful / Power
  'i-lucide-wallet',         // 26 E-Wallet
  'i-lucide-key-square',     // 27 Keys (variant)
  'i-lucide-notebook-text',  // 28 Notepad 2
  'i-lucide-id-card',        // 29 ID Card
  'i-lucide-nfc',            // 30 Smart Card
  'i-lucide-calculator',     // 31 Calculator
  'i-lucide-file-pen',       // 32 Notepad 3 / Edit
  'i-lucide-package',        // 33 Card Package
  'i-lucide-folder',         // 34 Folder
  'i-lucide-folder-open',    // 35 Folder Open
  'i-lucide-folder-archive', // 36 Folder Package
  'i-lucide-lock-open',      // 37 Lock Open
  'i-lucide-file-lock',      // 38 Paper Locked
  'i-lucide-circle-check',   // 39 Checked
  'i-lucide-pen',            // 40 Pen
  'i-lucide-image',          // 41 Thumbnail
  'i-lucide-book-open',      // 42 Book
  'i-lucide-list',           // 43 List
  'i-lucide-user-key',       // 44 User Key
  'i-lucide-wrench',         // 45 Tool
  'i-lucide-house',          // 46 Home
  'i-lucide-star',           // 47 Star
  'mdi:linux',               // 48 Tux / Linux  (no Lucide equivalent)
  'i-lucide-feather',        // 49 Feather
  'mdi:apple',               // 50 Apple         (no Lucide equivalent)
  'mdi:wikipedia',           // 51 Wikipedia      (no Lucide equivalent)
  'i-lucide-banknote',       // 52 Money
  'i-lucide-award',          // 53 Certificate
  'i-lucide-smartphone',     // 54 Phone / BlackBerry
  'i-lucide-tablet',         // 55 Palm / PDA
  'i-lucide-files',          // 56 Files
  'i-lucide-clipboard-check',// 57 Clipboard Check
  'i-lucide-file-archive',   // 58 Zip Archive
  'mdi:debian',              // 59 Debian / Linux (no Lucide equivalent)
  'mdi:firefox',             // 60 Firefox        (no Lucide equivalent)
  'mdi:google-chrome',       // 61 Chrome         (no Lucide equivalent)
  'mdi:internet-explorer',   // 62 Internet Explorer (no Lucide equivalent)
  'mdi:microsoft-windows',   // 63 Windows        (no Lucide equivalent)
  'i-lucide-screen-share',   // 64 Remote Desktop
  'i-lucide-timer',          // 65 Stopwatch
  'i-lucide-printer',        // 66 Printer
  'i-lucide-shield',         // 67 Emblem / Badge
  'i-lucide-camera',         // 68 Camera
]

function keepassStandardIcon(iconId: number | undefined): string | null {
  if (iconId === undefined) return null
  return KEEPASS_ICONS[iconId] ?? null
}

function getFieldValue(field: kdbxweb.KdbxEntryField | undefined): string {
  if (!field) return ''
  if (typeof field === 'string') return field
  if (field instanceof kdbxweb.ProtectedValue) return field.getText()
  return String(field)
}

function kdbxUuidToStandard(uuid: kdbxweb.KdbxUuid): string {
  const bin = atob(uuid.id)
  const bytes = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i)
  const hex = Array.from(bytes).map((b) => b.toString(16).padStart(2, '0')).join('')
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}

function uint8ToBase64(bytes: Uint8Array): string {
  let bin = ''
  for (let i = 0; i < bytes.length; i += 8192) {
    bin += String.fromCharCode(...Array.from(bytes.subarray(i, i + 8192)))
  }
  return btoa(bin)
}

interface BinaryLike { value?: kdbxweb.ProtectedValue | ArrayBuffer }

function extractBinaryData(b: kdbxweb.ProtectedValue | BinaryLike | ArrayBuffer): Uint8Array {
  if (b instanceof kdbxweb.ProtectedValue) return b.getBinary()
  if (b instanceof ArrayBuffer) return new Uint8Array(b)
  const inner = (b as BinaryLike).value
  if (inner instanceof kdbxweb.ProtectedValue) return inner.getBinary()
  if (inner instanceof ArrayBuffer) return new Uint8Array(inner)
  return new Uint8Array((b as unknown as ArrayBuffer))
}

interface ParsedOtp { secret: string; digits: number; period: number; algorithm: string }

function extractOtp(entry: kdbxweb.KdbxEntry, notes: string): ParsedOtp | null {
  const otpField = entry.fields.get('otp') ?? entry.fields.get('OTP')
  if (otpField) {
    const v = getFieldValue(otpField)
    if (v) {
      if (v.startsWith('otpauth://')) return parseOtpUri(v)
      return { secret: v.toUpperCase(), digits: 6, period: 30, algorithm: 'SHA1' }
    }
  }
  const seed = entry.fields.get('TOTP Seed') ?? entry.fields.get('totp-secret')
  if (seed) {
    const sv = getFieldValue(seed)
    if (sv) {
      const settings = getFieldValue(entry.fields.get('TOTP Settings') ?? entry.fields.get('totp-settings'))
      const parts = settings?.split(';') ?? []
      return {
        secret: sv.toUpperCase(),
        period: parseInt(parts[0] ?? '30', 10) || 30,
        digits: parseInt(parts[1] ?? '6', 10) || 6,
        algorithm: (parts[2] ?? 'SHA1').toUpperCase(),
      }
    }
  }
  if (notes) {
    const m = notes.match(/otpauth:\/\/totp\/[^\s]+/i)
    if (m) return parseOtpUri(m[0])
  }
  return null
}

function parseOtpUri(uri: string): ParsedOtp | null {
  try {
    const url = new URL(uri)
    const secret = url.searchParams.get('secret')
    if (!secret) return null
    return {
      secret: secret.toUpperCase(),
      digits: parseInt(url.searchParams.get('digits') ?? '6', 10),
      period: parseInt(url.searchParams.get('period') ?? '30', 10),
      algorithm: (url.searchParams.get('algorithm') ?? 'SHA1').toUpperCase(),
    }
  } catch { return null }
}

async function importKdbxAsync(buffer: ArrayBuffer, pwd: string): Promise<{ groupCount: number; entryCount: number }> {
  const creds = new kdbxweb.Credentials(kdbxweb.ProtectedValue.fromString(pwd))
  const kdbx = await kdbxweb.Kdbx.load(buffer, creds)

  const db = requireDb()
  const groupsStore = usePasswordsGroupsStore()
  const passwordsStore = usePasswordsStore()
  const tagsStore = usePasswordsTagsStore()

  const groupMap = new Map<string, string>()
  const standardFields = new Set(['Title', 'UserName', 'Password', 'URL', 'Notes'])

  const allGroups: Array<{ group: kdbxweb.KdbxGroup; parentUuid: string | null }> = []
  function collectGroups(group: kdbxweb.KdbxGroup, parentUuid: string | null = null) {
    if (group.name !== 'Root') allGroups.push({ group, parentUuid })
    for (const sub of group.groups) collectGroups(sub, kdbxUuidToStandard(group.uuid))
  }
  collectGroups(kdbx.getDefaultGroup())

  const allEntries = Array.from(kdbx.getDefaultGroup().allEntries())
  const total = allGroups.length + allEntries.length
  let step = 0

  for (const { group, parentUuid } of allGroups) {
    const groupUuid = kdbxUuidToStandard(group.uuid)
    const parentId = parentUuid ? (groupMap.get(parentUuid) ?? null) : null

    let icon: string | null = null
    if (group.customIcon?.id) {
      const iconData = kdbx.meta.customIcons.get(group.customIcon.id)
      if (iconData) {
        const base64 = uint8ToBase64(new Uint8Array(iconData.data))
        const hash = await addBinaryAsync(base64, iconData.data.byteLength, 'icon')
        icon = `binary:${hash}`
      }
    }
    if (!icon) icon = keepassStandardIcon(group.icon)

    const id = await groupsStore.addGroupAsync({
      id: groupUuid, name: group.name ?? '', icon, parentId: parentId ?? undefined,
    })
    groupMap.set(groupUuid, id)
    progress.value = Math.round((++step / total) * 100)
  }

  for (const entry of allEntries) {
    const parentGroupUuid = entry.parentGroup ? kdbxUuidToStandard(entry.parentGroup.uuid) : null
    const groupId = parentGroupUuid ? (groupMap.get(parentGroupUuid) ?? null) : null

    const title = getFieldValue(entry.fields.get('Title'))
    const username = getFieldValue(entry.fields.get('UserName'))
    const entryPassword = getFieldValue(entry.fields.get('Password'))
    const url = getFieldValue(entry.fields.get('URL'))
    const notes = getFieldValue(entry.fields.get('Notes'))
    const otp = extractOtp(entry, notes)

    let icon: string | null = null
    if (entry.customIcon?.id) {
      const iconData = kdbx.meta.customIcons.get(entry.customIcon.id)
      if (iconData) {
        const base64 = uint8ToBase64(new Uint8Array(iconData.data))
        const hash = await addBinaryAsync(base64, iconData.data.byteLength, 'icon')
        icon = `binary:${hash}`
      }
    }
    if (!icon) icon = keepassStandardIcon(entry.icon)

    const newId = kdbxUuidToStandard(entry.uuid)
    const createdAt = entry.times.creationTime ? new Date(entry.times.creationTime).toISOString() : new Date().toISOString()
    const updatedAt = entry.times.lastModTime ? new Date(entry.times.lastModTime).toISOString() : createdAt
    const expiresAt = entry.times.expires && entry.times.expiryTime
      ? new Date(entry.times.expiryTime).toISOString().split('T')[0] ?? null
      : null

    await db.insert(haexPasswordsItemDetails).values({
      id: newId, title, username: username || null, password: entryPassword || null,
      url: url || null, note: notes || null, icon,
      otpSecret: otp?.secret ?? null, otpDigits: otp?.digits ?? null,
      otpPeriod: otp?.period ?? null, otpAlgorithm: otp?.algorithm ?? null,
      expiresAt, createdAt, updatedAt,
    })

    await db.insert(haexPasswordsGroupItems).values({ itemId: newId, groupId })

    // Custom fields
    const kvEntries: Array<{ key: string; value: string }> = []
    for (const [key, value] of entry.fields) {
      if (!standardFields.has(key) && key !== 'otp' && key !== 'OTP' && !key.startsWith('TOTP')) {
        kvEntries.push({ key, value: getFieldValue(value) })
      }
    }
    if (kvEntries.length) {
      await db.insert(haexPasswordsItemKeyValues).values(
        kvEntries.map((kv) => ({ id: crypto.randomUUID(), itemId: newId, key: kv.key, value: kv.value })),
      )
    }

    // Tags
    const entryTags = entry.tags ?? []
    if (entryTags.length) {
      const tagRecords = await tagsStore.resolveTagNamesAsync(entryTags)
      await tagsStore.setItemTagsAsync(newId, tagRecords.map((t) => t.id))
    }

    // Attachments
    for (const [fileName, binary] of entry.binaries) {
      const bytes = extractBinaryData(binary as kdbxweb.ProtectedValue | BinaryLike | ArrayBuffer)
      if (!bytes.length) continue
      const base64 = uint8ToBase64(bytes)
      const hash = await addBinaryAsync(base64, bytes.length)
      await db.insert(haexPasswordsItemBinaries).values({
        id: crypto.randomUUID(), itemId: newId, binaryHash: hash, fileName,
      })
    }

    // History snapshots
    for (const histEntry of entry.history) {
      if (!histEntry) continue
      const histNotes = getFieldValue(histEntry.fields.get('Notes'))
      const histOtp = extractOtp(histEntry, histNotes)

      const snapshotData: SnapshotData = {
        title: getFieldValue(histEntry.fields.get('Title')),
        username: getFieldValue(histEntry.fields.get('UserName')) || null,
        password: getFieldValue(histEntry.fields.get('Password')) || null,
        url: getFieldValue(histEntry.fields.get('URL')) || null,
        note: histNotes || null,
        icon: null, color: null, expiresAt: null,
        otpSecret: histOtp?.secret ?? null,
        tagNames: histEntry.tags ?? [],
        keyValues: [],
        attachments: [],
      }

      for (const [key, value] of histEntry.fields) {
        if (!standardFields.has(key)) {
          snapshotData.keyValues.push({ key, value: getFieldValue(value) })
        }
      }

      const snapshotId = crypto.randomUUID()
      const snapCreatedAt = histEntry.times.creationTime
        ? new Date(histEntry.times.creationTime).toISOString()
        : new Date().toISOString()
      const snapModifiedAt = histEntry.times.lastModTime
        ? new Date(histEntry.times.lastModTime).toISOString()
        : null

      await db.insert(haexPasswordsItemSnapshots).values({
        id: snapshotId, itemId: newId,
        snapshotData: JSON.stringify(snapshotData),
        createdAt: snapCreatedAt, modifiedAt: snapModifiedAt,
      })

      for (const [fileName, binary] of histEntry.binaries) {
        const bytes = extractBinaryData(binary as kdbxweb.ProtectedValue | BinaryLike | ArrayBuffer)
        if (!bytes.length) continue
        const base64 = uint8ToBase64(bytes)
        const hash = await addBinaryAsync(base64, bytes.length)
        await db.insert(haexPasswordsSnapshotBinaries).values({
          id: crypto.randomUUID(), snapshotId, binaryHash: hash, fileName,
        })
      }
    }

    progress.value = Math.round((++step / total) * 100)
  }

  await groupsStore.loadGroupsAsync()
  await passwordsStore.loadItemsAsync()

  return { groupCount: allGroups.length, entryCount: allEntries.length }
}

watch(open, (v) => {
  if (!v) {
    fileData.value = null; selectedFileName.value = null; password.value = ''
    error.value = null; importing.value = false; progress.value = 0; showPassword.value = false
  }
})
</script>

<i18n lang="yaml">
de:
  title: KeePass Import
  selectFile: KeePass-Datei auswählen (.kdbx)
  kdbxFile: KDBX-Datei
  chooseFile: Datei auswählen
  password: Master-Passwort
  passwordPlaceholder: Gib dein KeePass Master-Passwort ein
  import: Importieren
  cancel: Abbrechen
  importing: Importiere
  error:
    parse: Fehler beim Lesen der Datei
    wrongPassword: Falsches Passwort
    noFile: Keine Datei ausgewählt
    noPassword: Bitte Master-Passwort eingeben
    import: Fehler beim Importieren
  success: Import erfolgreich
  successDescription: "{groups} Gruppen und {entries} Einträge wurden importiert"

en:
  title: KeePass Import
  selectFile: Select KeePass file (.kdbx)
  kdbxFile: KDBX File
  chooseFile: Choose file
  password: Master Password
  passwordPlaceholder: Enter your KeePass master password
  import: Import
  cancel: Cancel
  importing: Importing
  error:
    parse: Error reading file
    wrongPassword: Wrong password
    noFile: No file selected
    noPassword: Please enter master password
    import: Error importing data
  success: Import successful
  successDescription: "{groups} groups and {entries} entries imported"
</i18n>
