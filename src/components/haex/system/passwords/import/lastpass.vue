<template>
  <ImportWizardShell
    v-model:open="open"
    :title="t('title')"
    :description="t('selectFile')"
    :file-label="t('file')"
    :file-hint="t('fileHint')"
    accept=".csv"
    :success-title="t('success')"
    :success-description="successDescription"
    :error-import-label="t('error.import')"
    :error-no-file-label="t('error.noFile')"
    :do-import="doImport"
  />
</template>

<script setup lang="ts">
import {
  haexPasswordsItemDetails,
  haexPasswordsGroupItems,
  haexPasswordsItemKeyValues,
} from '~/database/schemas'
import { requireDb } from '~/stores/vault'
import { parseCSV } from '~/utils/csv'
import { parseOtpData } from '~/utils/passwords/otp'

const open = defineModel<boolean>('open', { default: false })
const { t } = useI18n()

interface LastPassCsvRow extends Record<string, string> {
  url: string
  username: string
  password: string
  totp: string
  extra: string
  name: string
  grouping: string
  fav: string
}

function parseExtraField(extra: string): Array<{ name: string; value: string }> {
  // LastPass crams secure-note fields into the `extra` column as `Key: Value`
  // lines. Heuristic: only treat the column as key-values when at least half
  // the lines look like `K: V` with a short key — otherwise it's free-form
  // notes and must not be silently shredded into structured key-values.
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

type ImportStats = { folderCount: number; entryCount: number }

const successDescription = (stats: ImportStats) =>
  t('successDescription', { folders: stats.folderCount, entries: stats.entryCount })

async function doImport(file: File, setProgress: (pct: number) => void): Promise<ImportStats> {
  const csvText = await file.text()
  // LastPass exports headers in mixed casing; lower-case them so the typed
  // row interface matches without per-row remapping.
  const rows = parseCSV<LastPassCsvRow>(csvText, h => h.toLowerCase())

  const db = requireDb()
  const groupsStore = usePasswordsGroupsStore()
  const passwordsStore = usePasswordsStore()

  const uniquePaths = new Set(rows.map(r => r.grouping?.trim()).filter(Boolean))
  // Process shorter paths first so parent folders exist before children try to
  // attach to them via parentId.
  const sortedPaths = Array.from(uniquePaths).sort(
    (a, b) => a.split('/').length - b.split('/').length,
  )

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
        const id = await groupsStore.addGroupAsync({
          name: part,
          icon: 'folder',
          parentId: parentId ?? undefined,
        })
        folderMap.set(currentPath, id)
      }
      parentId = folderMap.get(currentPath) ?? null
    }
    setProgress(Math.round((++step / total) * 100))
  }

  let entryCount = 0
  for (const row of rows) {
    const groupId = row.grouping?.trim() ? (folderMap.get(row.grouping.trim()) ?? null) : null
    const newId = crypto.randomUUID()
    const now = new Date().toISOString()

    if (row.url === 'http://sn') {
      // LastPass sentinel for a secure note — the row has no real URL.
      await db.insert(haexPasswordsItemDetails).values({
        id: newId,
        title: row.name || 'Secure Note',
        username: null,
        password: null,
        url: null,
        note: row.extra ?? null,
        icon: 'file-text',
        createdAt: now,
        updatedAt: now,
      })
    }
    else {
      // LastPass stores grouped Base32 secrets — strip whitespace.
      const otp = parseOtpData(row.totp, { stripWhitespace: true })
      await db.insert(haexPasswordsItemDetails).values({
        id: newId,
        title: row.name ?? '',
        username: row.username ?? null,
        password: row.password ?? null,
        url: row.url ?? null,
        note: row.extra ?? null,
        icon: row.fav === '1' ? 'star' : null,
        otpSecret: otp?.secret ?? null,
        otpDigits: otp?.digits ?? null,
        otpPeriod: otp?.period ?? null,
        otpAlgorithm: otp?.algorithm ?? null,
        createdAt: now,
        updatedAt: now,
      })
      const extraKv = parseExtraField(row.extra ?? '')
      if (extraKv.length) {
        await db.insert(haexPasswordsItemKeyValues).values(
          extraKv.map(f => ({ id: crypto.randomUUID(), itemId: newId, key: f.name, value: f.value })),
        )
      }
    }

    await db.insert(haexPasswordsGroupItems).values({ itemId: newId, groupId })
    entryCount++
    setProgress(Math.round((++step / total) * 100))
  }

  await groupsStore.loadGroupsAsync()
  await passwordsStore.loadItemsAsync()

  return { folderCount: new Set(folderMap.values()).size, entryCount }
}
</script>

<i18n lang="yaml">
de:
  title: LastPass Import
  selectFile: LastPass-Export auswählen (.csv)
  file: Export-Datei
  fileHint: "Exportiere deine Daten aus LastPass: Kontooptionen → Erweitert → Exportieren"
  error:
    noFile: Keine Datei ausgewählt
    import: Fehler beim Importieren
  success: Import erfolgreich
  successDescription: "{folders} Ordner und {entries} Einträge wurden importiert"

en:
  title: LastPass Import
  selectFile: Select LastPass export (.csv)
  file: Export File
  fileHint: "Export your data from LastPass: Account Options → Advanced → Export"
  error:
    noFile: No file selected
    import: Error importing data
  success: Import successful
  successDescription: "{folders} folders and {entries} entries imported"
</i18n>
