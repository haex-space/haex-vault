<template>
  <HaexSystemPasswordsImportWizardShell
    v-model:open="open"
    :title="t('title')"
    :description="t('selectFile')"
    :file-label="t('file')"
    :file-hint="t('fileHint')"
    accept=".csv,.json"
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
import { DEFAULT_AUTOFILL_ALIASES } from '~/utils/passwords/autofillAliases'

const open = defineModel<boolean>('open', { default: false })
const { t } = useI18n()

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

interface BitwardenCsvRow extends Record<string, string> {
  folder: string
  favorite: string
  type: string
  name: string
  notes: string
  fields: string
  login_uri: string
  login_username: string
  login_password: string
  login_totp: string
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

type ImportStats = { folderCount: number; entryCount: number }

const successDescription = (stats: ImportStats) =>
  t('successDescription', { folders: stats.folderCount, entries: stats.entryCount })

async function doImport(file: File, setProgress: (pct: number) => void): Promise<ImportStats> {
  const text = await file.text()
  if (file.name.endsWith('.json')) {
    return importJsonAsync(text, setProgress)
  }
  if (file.name.endsWith('.csv')) {
    return importCsvAsync(text, setProgress)
  }
  throw new Error(t('error.invalidFormat'))
}

async function importJsonAsync(jsonText: string, setProgress: (pct: number) => void): Promise<ImportStats> {
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
    setProgress(Math.round((++step / total) * 100))
  }

  for (const item of items) {
    const groupId = item.folderId ? (folderMap.get(item.folderId) ?? null) : null
    const newId = crypto.randomUUID()
    const now = new Date().toISOString()

    if (item.type === 1) {
      const otp = parseOtpData(item.login?.totp)
      await db.insert(haexPasswordsItemDetails).values({
        id: newId,
        title: item.name,
        username: item.login?.username ?? null,
        password: item.login?.password ?? null,
        url: item.login?.uris?.[0]?.uri ?? null,
        note: item.notes ?? null,
        icon: item.favorite ? 'star' : null,
        otpSecret: otp?.secret ?? null,
        otpDigits: otp?.digits ?? null,
        otpPeriod: otp?.period ?? null,
        otpAlgorithm: otp?.algorithm ?? null,
        autofillAliases: DEFAULT_AUTOFILL_ALIASES,
        createdAt: now,
        updatedAt: now,
      })
    }
    else if (item.type === 2) {
      await db.insert(haexPasswordsItemDetails).values({
        id: newId,
        title: item.name,
        username: null,
        password: null,
        url: null,
        note: item.notes ?? null,
        icon: 'file-text',
        autofillAliases: DEFAULT_AUTOFILL_ALIASES,
        createdAt: now,
        updatedAt: now,
      })
      const tag = await tagsStore.getOrCreateTagAsync('secure-note')
      await tagsStore.setItemTagsAsync(newId, [tag.id])
    }
    else if (item.type === 3) {
      await db.insert(haexPasswordsItemDetails).values({
        id: newId,
        title: item.name,
        username: item.card?.cardholderName ?? null,
        password: item.card?.number ?? null,
        url: null,
        note: item.notes ?? null,
        icon: 'credit-card',
        autofillAliases: DEFAULT_AUTOFILL_ALIASES,
        createdAt: now,
        updatedAt: now,
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
          cardFields.map(f => ({ id: crypto.randomUUID(), itemId: newId, key: f.key, value: f.value })),
        )
      }
    }
    else if (item.type === 4) {
      const fullName = [item.identity?.firstName, item.identity?.middleName, item.identity?.lastName].filter(Boolean).join(' ')
      await db.insert(haexPasswordsItemDetails).values({
        id: newId,
        title: item.name,
        username: item.identity?.username ?? item.identity?.email ?? null,
        password: null,
        url: null,
        note: item.notes ?? null,
        icon: 'user',
        autofillAliases: DEFAULT_AUTOFILL_ALIASES,
        createdAt: now,
        updatedAt: now,
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
          idFields.map(f => ({ id: crypto.randomUUID(), itemId: newId, key: f.key, value: f.value })),
        )
      }
    }
    else {
      setProgress(Math.round((++step / total) * 100))
      continue
    }

    await db.insert(haexPasswordsGroupItems).values({ itemId: newId, groupId })

    if (item.fields?.length) {
      await db.insert(haexPasswordsItemKeyValues).values(
        item.fields.map(f => ({ id: crypto.randomUUID(), itemId: newId, key: f.name, value: f.value })),
      )
    }
    if (item.type === 1 && (item.login?.uris?.length ?? 0) > 1) {
      await db.insert(haexPasswordsItemKeyValues).values(
        item.login!.uris!.slice(1).map((uri, idx) => ({
          id: crypto.randomUUID(),
          itemId: newId,
          key: `URL ${idx + 2}`,
          value: uri.uri,
        })),
      )
    }

    setProgress(Math.round((++step / total) * 100))
  }

  await groupsStore.loadGroupsAsync()
  await passwordsStore.loadItemsAsync()
  return { folderCount: folders.length, entryCount: items.length }
}

async function importCsvAsync(csvText: string, setProgress: (pct: number) => void): Promise<ImportStats> {
  const rows = parseCSV<BitwardenCsvRow>(csvText)
  const db = requireDb()
  const groupsStore = usePasswordsGroupsStore()
  const passwordsStore = usePasswordsStore()
  const tagsStore = usePasswordsTagsStore()

  const folderNames = new Set(rows.map(r => r.folder?.trim()).filter(Boolean))
  const folderMap = new Map<string, string>()
  const total = folderNames.size + rows.length
  let step = 0

  for (const name of folderNames) {
    const id = await groupsStore.addGroupAsync({ name, icon: 'folder' })
    folderMap.set(name, id)
    setProgress(Math.round((++step / total) * 100))
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
        id: newId,
        title: row.name ?? '',
        username: row.login_username ?? null,
        password: row.login_password ?? null,
        url: row.login_uri ?? null,
        note: row.notes ?? null,
        icon: row.favorite === '1' ? 'star' : null,
        otpSecret: otp?.secret ?? null,
        otpDigits: otp?.digits ?? null,
        otpPeriod: otp?.period ?? null,
        otpAlgorithm: otp?.algorithm ?? null,
        autofillAliases: DEFAULT_AUTOFILL_ALIASES,
        createdAt: now,
        updatedAt: now,
      })
    }
    else if (type === 'note' || type === 'securenote') {
      await db.insert(haexPasswordsItemDetails).values({
        id: newId,
        title: row.name || 'Secure Note',
        username: null,
        password: null,
        url: null,
        note: row.notes ?? null,
        icon: 'file-text',
        autofillAliases: DEFAULT_AUTOFILL_ALIASES,
        createdAt: now,
        updatedAt: now,
      })
      const tag = await tagsStore.getOrCreateTagAsync('secure-note')
      await tagsStore.setItemTagsAsync(newId, [tag.id])
    }
    else if (type === 'card') {
      await db.insert(haexPasswordsItemDetails).values({
        id: newId,
        title: row.name || 'Credit Card',
        username: null,
        password: null,
        url: null,
        note: row.notes ?? null,
        icon: 'credit-card',
        autofillAliases: DEFAULT_AUTOFILL_ALIASES,
        createdAt: now,
        updatedAt: now,
      })
      const tag = await tagsStore.getOrCreateTagAsync('credit-card')
      await tagsStore.setItemTagsAsync(newId, [tag.id])
    }
    else if (type === 'identity') {
      await db.insert(haexPasswordsItemDetails).values({
        id: newId,
        title: row.name || 'Identity',
        username: null,
        password: null,
        url: null,
        note: row.notes ?? null,
        icon: 'user',
        autofillAliases: DEFAULT_AUTOFILL_ALIASES,
        createdAt: now,
        updatedAt: now,
      })
      const tag = await tagsStore.getOrCreateTagAsync('identity')
      await tagsStore.setItemTagsAsync(newId, [tag.id])
    }
    else {
      setProgress(Math.round((++step / total) * 100))
      continue
    }

    await db.insert(haexPasswordsGroupItems).values({ itemId: newId, groupId })

    if (row.fields?.trim()) {
      const custom = parseCustomFieldsStr(row.fields)
      if (custom.length) {
        await db.insert(haexPasswordsItemKeyValues).values(
          custom.map(f => ({ id: crypto.randomUUID(), itemId: newId, key: f.name, value: f.value })),
        )
      }
    }

    entryCount++
    setProgress(Math.round((++step / total) * 100))
  }

  await groupsStore.loadGroupsAsync()
  await passwordsStore.loadItemsAsync()
  return { folderCount: folderNames.size, entryCount }
}
</script>

<i18n lang="yaml">
de:
  title: Bitwarden Import
  selectFile: Bitwarden-Export auswählen (.csv oder .json)
  file: Export-Datei
  fileHint: "Exportiere deine Daten aus Bitwarden: Einstellungen → Export Vault"
  error:
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
  fileHint: "Export your data from Bitwarden: Settings → Export Vault"
  error:
    noFile: No file selected
    invalidFormat: Invalid file format. Please select a .csv or .json file.
    encrypted: Encrypted exports are not supported. Please export without password.
    import: Error importing data
  success: Import successful
  successDescription: "{folders} folders and {entries} entries imported"
</i18n>
