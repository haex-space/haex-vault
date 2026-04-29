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
            accept=".csv"
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
const selectedFileName = ref<string | null>(null)
const importing = ref(false)
const progress = ref(0)
const error = ref<string | null>(null)

const canImport = computed(() => !!fileData.value && !importing.value)

const onFileChangeAsync = async (event: Event) => {
  const target = event.target as HTMLInputElement
  const file = target.files?.[0]
  if (!file) { selectedFileName.value = null; fileData.value = null; return }

  selectedFileName.value = file.name
  error.value = null

  if (!file.name.endsWith('.csv')) { error.value = t('error.invalidFormat'); return }

  try {
    fileData.value = await file.text()
  } catch (err) {
    error.value = t('error.parse')
    console.error(err)
  }
}

interface LastPassCsvRow {
  url: string; username: string; password: string; totp: string
  extra: string; name: string; grouping: string; fav: string
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

function parseCSV(csvText: string): LastPassCsvRow[] {
  const lines = csvText.split('\n')
  if (lines.length < 2) return []
  const header = parseCSVLine(lines[0] ?? '')
  return lines.slice(1)
    .filter((l) => l.trim())
    .map((line) => {
      const values = parseCSVLine(line)
      const row: Record<string, string> = {}
      header.forEach((col, idx) => { row[col.toLowerCase()] = values[idx] ?? '' })
      return row as unknown as LastPassCsvRow
    })
}

interface ParsedOtp { secret: string; digits: number; period: number; algorithm: string }

function parseOtpData(totp: string | null | undefined): ParsedOtp | null {
  if (!totp?.trim()) return null
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
  return { secret: totp.toUpperCase().replace(/\s/g, ''), digits: 6, period: 30, algorithm: 'SHA1' }
}

function parseExtraField(extra: string): Array<{ name: string; value: string }> {
  if (!extra || !extra.includes(':')) return []
  const lines = extra.split('\n')
  const kvLines = lines.filter((line) => {
    const idx = line.indexOf(':')
    return idx > 0 && idx < 30
  })
  if (kvLines.length < 2 || kvLines.length < lines.length * 0.5) return []
  return kvLines
    .map((line) => {
      const idx = line.indexOf(':')
      if (idx <= 0) return null
      const key = line.slice(0, idx).trim()
      const value = line.slice(idx + 1).trim()
      if (key === 'NoteType' || !value) return null
      return { name: key, value }
    })
    .filter((x): x is { name: string; value: string } => x !== null)
}

const importAsync = async () => {
  if (!fileData.value) { error.value = t('error.noFile'); return }
  importing.value = true; progress.value = 0; error.value = null
  try {
    const stats = await importCsvAsync(fileData.value)
    toast.add({
      title: t('success'),
      description: t('successDescription', { folders: stats.folderCount, entries: stats.entryCount }),
      color: 'success',
    })
    open.value = false
    fileData.value = null; selectedFileName.value = null
  } catch (err) {
    console.error('[LastPass Import]', err)
    error.value = t('error.import') + ': ' + (err instanceof Error ? err.message : String(err))
  } finally {
    importing.value = false; progress.value = 0
  }
}

async function importCsvAsync(csvText: string): Promise<{ folderCount: number; entryCount: number }> {
  const rows = parseCSV(csvText)
  const db = requireDb()
  const groupsStore = usePasswordsGroupsStore()
  const passwordsStore = usePasswordsStore()

  const uniquePaths = new Set(rows.map((r) => r.grouping?.trim()).filter(Boolean))
  const sortedPaths = Array.from(uniquePaths).sort((a, b) => a.split('/').length - b.split('/').length)

  const folderMap = new Map<string, string>()
  const total = sortedPaths.length + rows.length
  let step = 0

  for (const folderPath of sortedPaths) {
    const parts = folderPath.split('/')
    let currentPath = ''
    let parentId: string | null = null
    for (const part of parts) {
      currentPath = currentPath ? `${currentPath}/${part}` : part
      if (!folderMap.has(currentPath)) {
        const id = await groupsStore.addGroupAsync({ name: part, icon: 'folder', parentId: parentId ?? undefined })
        folderMap.set(currentPath, id)
      }
      parentId = folderMap.get(currentPath) ?? null
    }
    progress.value = Math.round((++step / total) * 100)
  }

  let entryCount = 0
  for (const row of rows) {
    const groupId = row.grouping?.trim() ? (folderMap.get(row.grouping.trim()) ?? null) : null
    const newId = crypto.randomUUID()
    const now = new Date().toISOString()

    if (row.url === 'http://sn') {
      await db.insert(haexPasswordsItemDetails).values({
        id: newId, title: row.name || 'Secure Note', username: null, password: null, url: null,
        note: row.extra ?? null, icon: 'file-text', createdAt: now, updatedAt: now,
      })
    } else {
      const otp = parseOtpData(row.totp)
      await db.insert(haexPasswordsItemDetails).values({
        id: newId, title: row.name ?? '', username: row.username ?? null,
        password: row.password ?? null, url: row.url ?? null,
        note: row.extra ?? null, icon: row.fav === '1' ? 'star' : null,
        otpSecret: otp?.secret ?? null, otpDigits: otp?.digits ?? null,
        otpPeriod: otp?.period ?? null, otpAlgorithm: otp?.algorithm ?? null,
        createdAt: now, updatedAt: now,
      })
      const extraKv = parseExtraField(row.extra ?? '')
      if (extraKv.length) {
        await db.insert(haexPasswordsItemKeyValues).values(
          extraKv.map((f) => ({ id: crypto.randomUUID(), itemId: newId, key: f.name, value: f.value })),
        )
      }
    }

    await db.insert(haexPasswordsGroupItems).values({ itemId: newId, groupId })
    entryCount++
    progress.value = Math.round((++step / total) * 100)
  }

  await groupsStore.loadGroupsAsync()
  await passwordsStore.loadItemsAsync()

  return { folderCount: new Set(folderMap.values()).size, entryCount }
}

watch(open, (v) => {
  if (!v) {
    fileData.value = null; selectedFileName.value = null
    error.value = null; importing.value = false; progress.value = 0
  }
})
</script>

<i18n lang="yaml">
de:
  title: LastPass Import
  selectFile: LastPass-Export auswählen (.csv)
  file: Export-Datei
  chooseFile: Datei auswählen
  fileHint: "Exportiere deine Daten aus LastPass: Kontooptionen → Erweitert → Exportieren"
  import: Importieren
  cancel: Abbrechen
  importing: Importiere
  error:
    parse: Fehler beim Lesen der Datei
    noFile: Keine Datei ausgewählt
    invalidFormat: Ungültiges Dateiformat. Bitte .csv Datei auswählen.
    import: Fehler beim Importieren
  success: Import erfolgreich
  successDescription: "{folders} Ordner und {entries} Einträge wurden importiert"

en:
  title: LastPass Import
  selectFile: Select LastPass export (.csv)
  file: Export File
  chooseFile: Choose file
  fileHint: "Export your data from LastPass: Account Options → Advanced → Export"
  import: Import
  cancel: Cancel
  importing: Importing
  error:
    parse: Error reading file
    noFile: No file selected
    invalidFormat: Invalid file format. Please select a .csv file.
    import: Error importing data
  success: Import successful
  successDescription: "{folders} folders and {entries} entries imported"
</i18n>
