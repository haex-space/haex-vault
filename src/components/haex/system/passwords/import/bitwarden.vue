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
            {{ t('file') }}
          </p>
          <input
            ref="fileInput"
            type="file"
            accept=".csv,.json"
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
          <p class="text-xs text-muted">
            {{ t('fileHint') }}
          </p>
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
import { useToast } from '#imports'
import {
  haexPasswordsItemDetails,
  haexPasswordsGroupItems,
  haexPasswordsItemKeyValues,
} from '~/database/schemas'
import { requireDb } from '~/stores/vault'

const open = defineModel<boolean>('open', { default: false })
const { t } = useI18n()
const toast = useToast()

const fileInput = useTemplateRef<HTMLInputElement>('fileInput')
const fileData = ref<string | null>(null)
const fileType = ref<'csv' | 'json' | null>(null)
const selectedFileName = ref<string | null>(null)
const importing = ref(false)
const progress = ref(0)
const error = ref<string | null>(null)

const canImport = computed(() => !!fileData.value && !importing.value)

const onFileChangeAsync = async (event: Event) => {
  const target = event.target as HTMLInputElement
  const file = target.files?.[0]
  if (!file) { selectedFileName.value = null; fileData.value = null; fileType.value = null; return }

  selectedFileName.value = file.name
  error.value = null

  if (file.name.endsWith('.json')) {
    fileType.value = 'json'
  } else if (file.name.endsWith('.csv')) {
    fileType.value = 'csv'
  } else {
    error.value = t('error.invalidFormat')
    return
  }

  try {
    fileData.value = await file.text()
  } catch (err) {
    error.value = t('error.parse')
    console.error(err)
  }
}

interface BitwardenJsonExport {
  encrypted?: boolean
  folders?: Array<{ id: string; name: string }>
  items?: Array<{
    id: string
    folderId?: string | null
    type: number
    name: string
    notes?: string | null
    favorite: boolean
    login?: {
      uris?: Array<{ uri: string }>
      username?: string | null
      password?: string | null
      totp?: string | null
    }
    card?: {
      cardholderName?: string | null
      brand?: string | null
      number?: string | null
      expMonth?: string | null
      expYear?: string | null
      code?: string | null
    }
    identity?: {
      title?: string | null
      firstName?: string | null
      middleName?: string | null
      lastName?: string | null
      address1?: string | null
      address2?: string | null
      address3?: string | null
      city?: string | null
      state?: string | null
      postalCode?: string | null
      country?: string | null
      company?: string | null
      email?: string | null
      phone?: string | null
      ssn?: string | null
      username?: string | null
      passportNumber?: string | null
      licenseNumber?: string | null
    }
    fields?: Array<{ name: string; value: string; type: number }>
  }>
}

interface BitwardenCsvRow {
  folder: string; favorite: string; type: string; name: string
  notes: string; fields: string; login_uri: string
  login_username: string; login_password: string; login_totp: string
}

function parseCSVLine(line: string): string[] {
  const result: string[] = []
  let current = ''
  let inQuotes = false
  for (let i = 0; i < line.length; i++) {
    const char = line[i]!
    const next = line[i + 1]
    if (inQuotes) {
      if (char === '"' && next === '"') { current += '"'; i++ }
      else if (char === '"') { inQuotes = false }
      else { current += char }
    } else {
      if (char === '"') { inQuotes = true }
      else if (char === ',') { result.push(current); current = '' }
      else { current += char }
    }
  }
  result.push(current)
  return result
}

function parseCSV(csvText: string): BitwardenCsvRow[] {
  const lines = csvText.split('\n')
  if (lines.length < 2) return []
  const header = parseCSVLine(lines[0] ?? '')
  return lines.slice(1)
    .filter((l) => l.trim())
    .map((line) => {
      const values = parseCSVLine(line)
      const row: Record<string, string> = {}
      header.forEach((col, idx) => { row[col] = values[idx] ?? '' })
      return row as unknown as BitwardenCsvRow
    })
}

function parseCustomFieldsStr(fieldsStr: string): Array<{ name: string; value: string }> {
  return fieldsStr.split('\n')
    .map((line) => {
      const idx = line.indexOf(':')
      if (idx <= 0) return null
      return { name: line.slice(0, idx).trim(), value: line.slice(idx + 1).trim() }
    })
    .filter((x): x is { name: string; value: string } => x !== null)
}

interface ParsedOtp { secret: string; digits: number; period: number; algorithm: string }

function parseOtpData(totp: string | null | undefined): ParsedOtp | null {
  if (!totp) return null
  if (totp.startsWith('otpauth://')) {
    try {
      const url = new URL(totp)
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
  return { secret: totp.toUpperCase(), digits: 6, period: 30, algorithm: 'SHA1' }
}

const importAsync = async () => {
  if (!fileData.value || !fileType.value) { error.value = t('error.noFile'); return }
  importing.value = true; progress.value = 0; error.value = null
  try {
    const stats = fileType.value === 'json'
      ? await importJsonAsync(fileData.value)
      : await importCsvAsync(fileData.value)
    toast.add({
      title: t('success'),
      description: t('successDescription', { folders: stats.folderCount, entries: stats.entryCount }),
      color: 'success',
    })
    open.value = false
    fileData.value = null; selectedFileName.value = null; fileType.value = null
  } catch (err) {
    console.error('[Bitwarden Import]', err)
    error.value = t('error.import') + ': ' + (err instanceof Error ? err.message : String(err))
  } finally {
    importing.value = false; progress.value = 0
  }
}

async function importJsonAsync(jsonText: string): Promise<{ folderCount: number; entryCount: number }> {
  const data: BitwardenJsonExport = JSON.parse(jsonText)
  if (data.encrypted) throw new Error(t('error.encrypted'))

  const db = requireDb()
  const groupsStore = usePasswordsGroupsStore()
  const passwordsStore = usePasswordsStore()
  const tagsStore = usePasswordsTagsStore()

  const folderMap = new Map<string, string>()
  const folders = data.folders ?? []
  const items = data.items ?? []
  const total = folders.length + items.length
  let step = 0

  for (const folder of folders) {
    const id = await groupsStore.addGroupAsync({ name: folder.name, icon: 'folder' })
    folderMap.set(folder.id, id)
    progress.value = Math.round((++step / total) * 100)
  }

  for (const item of items) {
    const groupId = item.folderId ? (folderMap.get(item.folderId) ?? null) : null
    const newId = crypto.randomUUID()
    const now = new Date().toISOString()

    if (item.type === 1) {
      const otp = parseOtpData(item.login?.totp)
      await db.insert(haexPasswordsItemDetails).values({
        id: newId, title: item.name,
        username: item.login?.username ?? null,
        password: item.login?.password ?? null,
        url: item.login?.uris?.[0]?.uri ?? null,
        note: item.notes ?? null, icon: item.favorite ? 'star' : null,
        otpSecret: otp?.secret ?? null, otpDigits: otp?.digits ?? null,
        otpPeriod: otp?.period ?? null, otpAlgorithm: otp?.algorithm ?? null,
        createdAt: now, updatedAt: now,
      })
    } else if (item.type === 2) {
      await db.insert(haexPasswordsItemDetails).values({
        id: newId, title: item.name, username: null, password: null, url: null,
        note: item.notes ?? null, icon: 'file-text', createdAt: now, updatedAt: now,
      })
      const tag = await tagsStore.getOrCreateTagAsync('secure-note')
      await tagsStore.setItemTagsAsync(newId, [tag.id])
    } else if (item.type === 3) {
      await db.insert(haexPasswordsItemDetails).values({
        id: newId, title: item.name,
        username: item.card?.cardholderName ?? null,
        password: item.card?.number ?? null, url: null,
        note: item.notes ?? null, icon: 'credit-card', createdAt: now, updatedAt: now,
      })
      const tag = await tagsStore.getOrCreateTagAsync('credit-card')
      await tagsStore.setItemTagsAsync(newId, [tag.id])
      const cardFields: Array<{ key: string; value: string }> = []
      if (item.card?.brand) cardFields.push({ key: 'Brand', value: item.card.brand })
      if (item.card?.expMonth) cardFields.push({ key: 'Expiration Month', value: item.card.expMonth })
      if (item.card?.expYear) cardFields.push({ key: 'Expiration Year', value: item.card.expYear })
      if (item.card?.code) cardFields.push({ key: 'CVV', value: item.card.code })
      if (cardFields.length) {
        await db.insert(haexPasswordsItemKeyValues).values(
          cardFields.map((f) => ({ id: crypto.randomUUID(), itemId: newId, key: f.key, value: f.value })),
        )
      }
    } else if (item.type === 4) {
      const fullName = [item.identity?.firstName, item.identity?.middleName, item.identity?.lastName].filter(Boolean).join(' ')
      await db.insert(haexPasswordsItemDetails).values({
        id: newId, title: item.name,
        username: item.identity?.username ?? item.identity?.email ?? null,
        password: null, url: null, note: item.notes ?? null, icon: 'user', createdAt: now, updatedAt: now,
      })
      const tag = await tagsStore.getOrCreateTagAsync('identity')
      await tagsStore.setItemTagsAsync(newId, [tag.id])
      const idFields: Array<{ key: string; value: string }> = []
      if (fullName) idFields.push({ key: 'Full Name', value: fullName })
      if (item.identity?.email) idFields.push({ key: 'Email', value: item.identity.email })
      if (item.identity?.phone) idFields.push({ key: 'Phone', value: item.identity.phone })
      if (item.identity?.company) idFields.push({ key: 'Company', value: item.identity.company })
      const addr = [item.identity?.address1, item.identity?.address2, item.identity?.address3].filter(Boolean)
      if (addr.length) idFields.push({ key: 'Address', value: addr.join('\n') })
      if (item.identity?.city) idFields.push({ key: 'City', value: item.identity.city })
      if (item.identity?.country) idFields.push({ key: 'Country', value: item.identity.country })
      if (idFields.length) {
        await db.insert(haexPasswordsItemKeyValues).values(
          idFields.map((f) => ({ id: crypto.randomUUID(), itemId: newId, key: f.key, value: f.value })),
        )
      }
    } else {
      progress.value = Math.round((++step / total) * 100)
      continue
    }

    await db.insert(haexPasswordsGroupItems).values({ itemId: newId, groupId })

    if (item.fields?.length) {
      await db.insert(haexPasswordsItemKeyValues).values(
        item.fields.map((f) => ({ id: crypto.randomUUID(), itemId: newId, key: f.name, value: f.value })),
      )
    }
    if (item.type === 1 && (item.login?.uris?.length ?? 0) > 1) {
      await db.insert(haexPasswordsItemKeyValues).values(
        item.login!.uris!.slice(1).map((uri, idx) => ({
          id: crypto.randomUUID(), itemId: newId, key: `URL ${idx + 2}`, value: uri.uri,
        })),
      )
    }

    progress.value = Math.round((++step / total) * 100)
  }

  await groupsStore.loadGroupsAsync()
  await passwordsStore.loadItemsAsync()
  return { folderCount: folders.length, entryCount: items.length }
}

async function importCsvAsync(csvText: string): Promise<{ folderCount: number; entryCount: number }> {
  const rows = parseCSV(csvText)
  const db = requireDb()
  const groupsStore = usePasswordsGroupsStore()
  const passwordsStore = usePasswordsStore()
  const tagsStore = usePasswordsTagsStore()

  const folderNames = new Set(rows.map((r) => r.folder?.trim()).filter(Boolean))
  const folderMap = new Map<string, string>()
  const total = folderNames.size + rows.length
  let step = 0

  for (const name of folderNames) {
    const id = await groupsStore.addGroupAsync({ name, icon: 'folder' })
    folderMap.set(name, id)
    progress.value = Math.round((++step / total) * 100)
  }

  let entryCount = 0
  for (const row of rows) {
    const groupId = row.folder?.trim() ? (folderMap.get(row.folder.trim()) ?? null) : null
    const newId = crypto.randomUUID()
    const now = new Date().toISOString()
    const type = (row.type || 'login').toLowerCase()

    if (type === 'login') {
      const otp = parseOtpData(row.login_totp)
      await db.insert(haexPasswordsItemDetails).values({
        id: newId, title: row.name ?? '', username: row.login_username ?? null,
        password: row.login_password ?? null, url: row.login_uri ?? null,
        note: row.notes ?? null, icon: row.favorite === '1' ? 'star' : null,
        otpSecret: otp?.secret ?? null, otpDigits: otp?.digits ?? null,
        otpPeriod: otp?.period ?? null, otpAlgorithm: otp?.algorithm ?? null,
        createdAt: now, updatedAt: now,
      })
    } else if (type === 'note' || type === 'securenote') {
      await db.insert(haexPasswordsItemDetails).values({
        id: newId, title: row.name || 'Secure Note', username: null, password: null, url: null,
        note: row.notes ?? null, icon: 'file-text', createdAt: now, updatedAt: now,
      })
      const tag = await tagsStore.getOrCreateTagAsync('secure-note')
      await tagsStore.setItemTagsAsync(newId, [tag.id])
    } else if (type === 'card') {
      await db.insert(haexPasswordsItemDetails).values({
        id: newId, title: row.name || 'Credit Card', username: null, password: null, url: null,
        note: row.notes ?? null, icon: 'credit-card', createdAt: now, updatedAt: now,
      })
      const tag = await tagsStore.getOrCreateTagAsync('credit-card')
      await tagsStore.setItemTagsAsync(newId, [tag.id])
    } else if (type === 'identity') {
      await db.insert(haexPasswordsItemDetails).values({
        id: newId, title: row.name || 'Identity', username: null, password: null, url: null,
        note: row.notes ?? null, icon: 'user', createdAt: now, updatedAt: now,
      })
      const tag = await tagsStore.getOrCreateTagAsync('identity')
      await tagsStore.setItemTagsAsync(newId, [tag.id])
    } else {
      progress.value = Math.round((++step / total) * 100)
      continue
    }

    await db.insert(haexPasswordsGroupItems).values({ itemId: newId, groupId })

    if (row.fields?.trim()) {
      const custom = parseCustomFieldsStr(row.fields)
      if (custom.length) {
        await db.insert(haexPasswordsItemKeyValues).values(
          custom.map((f) => ({ id: crypto.randomUUID(), itemId: newId, key: f.name, value: f.value })),
        )
      }
    }

    entryCount++
    progress.value = Math.round((++step / total) * 100)
  }

  await groupsStore.loadGroupsAsync()
  await passwordsStore.loadItemsAsync()
  return { folderCount: folderNames.size, entryCount }
}

watch(open, (v) => {
  if (!v) {
    fileData.value = null; selectedFileName.value = null
    fileType.value = null; error.value = null
    importing.value = false; progress.value = 0
  }
})
</script>

<i18n lang="yaml">
de:
  title: Bitwarden Import
  selectFile: Bitwarden-Export auswählen (.csv oder .json)
  file: Export-Datei
  chooseFile: Datei auswählen
  fileHint: "Exportiere deine Daten aus Bitwarden: Einstellungen → Export Vault"
  import: Importieren
  cancel: Abbrechen
  importing: Importiere
  error:
    parse: Fehler beim Lesen der Datei
    noFile: Keine Datei ausgewählt
    invalidFormat: Ungültiges Dateiformat. Bitte .csv oder .json Datei auswählen.
    encrypted: Verschlüsselte Exporte werden nicht unterstützt. Bitte exportiere ohne Passwort.
    import: Fehler beim Importieren
  success: Import erfolgreich
  successDescription: "{folders} Ordner und {entries} Einträge wurden importiert"

en:
  title: Bitwarden Import
  selectFile: Select Bitwarden export (.csv or .json)
  file: Export File
  chooseFile: Choose file
  fileHint: "Export your data from Bitwarden: Settings → Export Vault"
  import: Import
  cancel: Cancel
  importing: Importing
  error:
    parse: Error reading file
    noFile: No file selected
    invalidFormat: Invalid file format. Please select a .csv or .json file.
    encrypted: Encrypted exports are not supported. Please export without password.
    import: Error importing data
  success: Import successful
  successDescription: "{folders} folders and {entries} entries imported"
</i18n>
