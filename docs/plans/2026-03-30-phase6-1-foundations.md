# Phase 6.1: Foundations for Local Spaces

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add database schema, settings, and frontend protections needed before building the P2P transport layer.

**Architecture:** New `_no_sync` tables for local delivery service buffering, `leader_priority` on space devices, housekeeping settings in vault-settings enum, and frontend protection (vault-space hidden, space type immutable). The `DecryptedSpace` type from vault-sdk lacks a `type` field — we extend it locally.

**Tech Stack:** Drizzle ORM (SQLite), Vue 3 + Pinia, TypeScript, Tauri

---

### Task 1: Add `leaderPriority` column to `haexSpaceDevices`

**Files:**
- Modify: `src/database/tableNames.json` (add `leaderPriority` column to `space_devices`)
- Modify: `src/database/schemas/haex.ts` (add column to `haexSpaceDevices`)

**Step 1: Add column to tableNames.json**

In `src/database/tableNames.json`, in the `space_devices.columns` object, add:

```json
"leaderPriority": "leader_priority"
```

After the existing `relayUrl` entry.

**Step 2: Add column to Drizzle schema**

In `src/database/schemas/haex.ts`, add to the `haexSpaceDevices` table definition, after the `relayUrl` column:

```typescript
leaderPriority: integer(tableNames.haex.space_devices.columns.leaderPriority).default(10),
```

**Step 3: Generate migration**

Run: `cd /home/haex/Projekte/haex-vault && pnpm drizzle:generate`

This creates a new migration file in `src-tauri/database/migrations/`.

**Step 4: Verify migration**

Check the generated migration SQL contains:
```sql
ALTER TABLE haex_space_devices ADD leader_priority integer DEFAULT 10;
```

**Step 5: Commit**

```
feat: add leader_priority column to space devices
```

---

### Task 2: Add local delivery service buffer tables to tableNames.json

**Files:**
- Modify: `src/database/tableNames.json`

**Step 1: Add 4 new table definitions**

In `src/database/tableNames.json`, in the `haex` object, add after the `device_mls_enrollments` entry:

```json
"local_ds_messages": {
  "name": "haex_local_ds_messages_no_sync",
  "columns": {
    "id": "id",
    "spaceId": "space_id",
    "senderDid": "sender_did",
    "messageType": "message_type",
    "messageBlob": "message_blob",
    "createdAt": "created_at"
  }
},
"local_ds_key_packages": {
  "name": "haex_local_ds_key_packages_no_sync",
  "columns": {
    "id": "id",
    "spaceId": "space_id",
    "targetDid": "target_did",
    "packageBlob": "package_blob",
    "createdAt": "created_at"
  }
},
"local_ds_welcomes": {
  "name": "haex_local_ds_welcomes_no_sync",
  "columns": {
    "id": "id",
    "spaceId": "space_id",
    "recipientDid": "recipient_did",
    "welcomeBlob": "welcome_blob",
    "consumed": "consumed",
    "createdAt": "created_at"
  }
},
"local_ds_pending_commits": {
  "name": "haex_local_ds_pending_commits_no_sync",
  "columns": {
    "id": "id",
    "spaceId": "space_id",
    "commitBlob": "commit_blob",
    "deliveredTo": "delivered_to",
    "createdAt": "created_at"
  }
}
```

**Step 2: Verify JSON validity**

Run: `cd /home/haex/Projekte/haex-vault && node -e "JSON.parse(require('fs').readFileSync('src/database/tableNames.json','utf8')); console.log('Valid JSON')"`

Expected: `Valid JSON`

**Step 3: Commit**

```
feat: add local delivery service table definitions to tableNames.json
```

---

### Task 3: Create Drizzle schema for local delivery tables

**Files:**
- Create: `src/database/schemas/localDelivery.ts`
- Modify: `src/database/schemas/index.ts`

**Step 1: Create the schema file**

Create `src/database/schemas/localDelivery.ts`:

```typescript
import { sqliteTable, text, integer, index, blob } from 'drizzle-orm/sqlite-core'
import { sql } from 'drizzle-orm'
import tableNames from '../tableNames.json'
import { haexSpaces } from './haex'

// ---------------------------------------------------------------------------
// Local Delivery Service — MLS message buffering for local spaces (_no_sync)
// ---------------------------------------------------------------------------

export const haexLocalDsMessages = sqliteTable(
  tableNames.haex.local_ds_messages.name,
  {
    id: integer(tableNames.haex.local_ds_messages.columns.id).primaryKey({ autoIncrement: true }),
    spaceId: text(tableNames.haex.local_ds_messages.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id),
    senderDid: text(tableNames.haex.local_ds_messages.columns.senderDid).notNull(),
    messageType: text(tableNames.haex.local_ds_messages.columns.messageType).notNull(), // 'commit' | 'proposal' | 'application'
    messageBlob: blob(tableNames.haex.local_ds_messages.columns.messageBlob, { mode: 'buffer' }).notNull(),
    createdAt: text(tableNames.haex.local_ds_messages.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
  (table) => [
    index('haex_local_ds_messages_space_idx').on(table.spaceId),
  ],
)
export type InsertHaexLocalDsMessages = typeof haexLocalDsMessages.$inferInsert
export type SelectHaexLocalDsMessages = typeof haexLocalDsMessages.$inferSelect

export const haexLocalDsKeyPackages = sqliteTable(
  tableNames.haex.local_ds_key_packages.name,
  {
    id: text(tableNames.haex.local_ds_key_packages.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    spaceId: text(tableNames.haex.local_ds_key_packages.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id),
    targetDid: text(tableNames.haex.local_ds_key_packages.columns.targetDid).notNull(),
    packageBlob: blob(tableNames.haex.local_ds_key_packages.columns.packageBlob, { mode: 'buffer' }).notNull(),
    createdAt: text(tableNames.haex.local_ds_key_packages.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
  (table) => [
    index('haex_local_ds_key_packages_space_did_idx').on(table.spaceId, table.targetDid),
  ],
)
export type InsertHaexLocalDsKeyPackages = typeof haexLocalDsKeyPackages.$inferInsert
export type SelectHaexLocalDsKeyPackages = typeof haexLocalDsKeyPackages.$inferSelect

export const haexLocalDsWelcomes = sqliteTable(
  tableNames.haex.local_ds_welcomes.name,
  {
    id: text(tableNames.haex.local_ds_welcomes.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    spaceId: text(tableNames.haex.local_ds_welcomes.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id),
    recipientDid: text(tableNames.haex.local_ds_welcomes.columns.recipientDid).notNull(),
    welcomeBlob: blob(tableNames.haex.local_ds_welcomes.columns.welcomeBlob, { mode: 'buffer' }).notNull(),
    consumed: integer(tableNames.haex.local_ds_welcomes.columns.consumed).default(0),
    createdAt: text(tableNames.haex.local_ds_welcomes.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
  (table) => [
    index('haex_local_ds_welcomes_recipient_idx').on(table.spaceId, table.recipientDid),
  ],
)
export type InsertHaexLocalDsWelcomes = typeof haexLocalDsWelcomes.$inferInsert
export type SelectHaexLocalDsWelcomes = typeof haexLocalDsWelcomes.$inferSelect

export const haexLocalDsPendingCommits = sqliteTable(
  tableNames.haex.local_ds_pending_commits.name,
  {
    id: text(tableNames.haex.local_ds_pending_commits.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    spaceId: text(tableNames.haex.local_ds_pending_commits.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id),
    commitBlob: blob(tableNames.haex.local_ds_pending_commits.columns.commitBlob, { mode: 'buffer' }).notNull(),
    deliveredTo: text(tableNames.haex.local_ds_pending_commits.columns.deliveredTo).default('[]'), // JSON array of endpoint_ids
    createdAt: text(tableNames.haex.local_ds_pending_commits.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
  (table) => [
    index('haex_local_ds_pending_commits_space_idx').on(table.spaceId),
  ],
)
export type InsertHaexLocalDsPendingCommits = typeof haexLocalDsPendingCommits.$inferInsert
export type SelectHaexLocalDsPendingCommits = typeof haexLocalDsPendingCommits.$inferSelect
```

**Step 2: Export from index**

In `src/database/schemas/index.ts`, add:

```typescript
export * from './localDelivery'
```

**Step 3: Generate migration**

Run: `cd /home/haex/Projekte/haex-vault && pnpm drizzle:generate`

**Step 4: Verify migration**

Check that the generated migration creates all 4 `haex_local_ds_*_no_sync` tables with correct columns and indexes.

**Step 5: Commit**

```
feat: add local delivery service buffer tables
```

---

### Task 4: Add housekeeping settings keys

**Files:**
- Modify: `src/config/vault-settings.ts`

**Step 1: Add new enum values**

In `src/config/vault-settings.ts`, add to `VaultSettingsKeyEnum` after the `logRetentionDays` entry:

```typescript
localDsMessageTtlDays = 'local_ds_message_ttl_days',
localDsKeyPackageTtlHours = 'local_ds_key_package_ttl_hours',
localDsWelcomeTtlDays = 'local_ds_welcome_ttl_days',
localDsPendingCommitTtlHours = 'local_ds_pending_commit_ttl_hours',
localDsCleanupIntervalMinutes = 'local_ds_cleanup_interval_minutes',
```

**Step 2: Commit**

```
feat: add local delivery housekeeping settings keys
```

---

### Task 5: Create local `SpaceWithType` type and filter vault spaces

The SDK's `DecryptedSpace` type lacks a `type` field. We create a local extension that includes it, and use it in the spaces store.

**Files:**
- Modify: `src/stores/spaces.ts`

**Step 1: Add local type and filter computed**

At the top of `src/stores/spaces.ts`, after the existing imports, add a local type:

```typescript
/** Extended space type that includes the DB type field (vault/shared/local) */
export interface SpaceWithType extends DecryptedSpace {
  type: 'vault' | 'shared' | 'local'
}
```

**Step 2: Change spaces ref type**

Change:
```typescript
const spaces = ref<DecryptedSpace[]>([])
```
To:
```typescript
const spaces = ref<SpaceWithType[]>([])
```

**Step 3: Update `rowToDecryptedSpace` to include type**

Rename to `rowToSpace` and include the `type` field:

```typescript
const rowToSpace = (row: SelectHaexSpaces): SpaceWithType => ({
  id: row.id,
  name: row.name,
  type: (row.type as SpaceWithType['type']) ?? 'shared',
  role: row.role as SpaceRole,
  serverUrl: row.serverUrl ?? '',
  createdAt: row.createdAt ?? '',
})
```

Update all call sites of `rowToDecryptedSpace` to `rowToSpace` (2 places: `loadSpacesFromDbAsync` line 34 and `ensureDefaultSpaceAsync` line 169).

**Step 4: Add computed for visible spaces**

Inside the store, add a computed that filters out vault spaces:

```typescript
/** Spaces visible in the UI (excludes vault spaces) */
const visibleSpaces = computed(() => spaces.value.filter(s => s.type !== 'vault'))
```

Export `visibleSpaces` from the store return object.

**Step 5: Update `createLocalSpaceAsync` to include type**

In `createLocalSpaceAsync`, change the space object to include `type: 'local'`:

```typescript
const space: SpaceWithType = {
  id,
  name: spaceName,
  type: 'local',
  role: SpaceRoles.ADMIN,
  serverUrl: '',
  createdAt: new Date().toISOString(),
}
```

**Step 6: Update `listSpacesAsync` mapped spaces to include type**

In `listSpacesAsync`, the `decrypted` array mapping (around line 302) should include type:

```typescript
const decrypted: SpaceWithType[] = rawSpaces.map((space) => ({
  id: space.id,
  name: space.encryptedName ?? `Space ${space.id.slice(0, 8)}`,
  type: 'shared' as const,
  role: space.role,
  serverUrl,
  createdAt: space.createdAt,
}))
```

**Step 7: Update `claimInviteTokenAsync` to include type**

In `claimInviteTokenAsync`, the space object (around line 447) should include type:

```typescript
const space: SpaceWithType = {
  id: spaceId,
  name: '',
  type: 'shared',
  role: data.capability === 'space/admin' ? SpaceRoles.ADMIN : data.capability === 'space/read' ? SpaceRoles.READER : SpaceRoles.MEMBER,
  serverUrl,
  createdAt: new Date().toISOString(),
}
```

**Step 8: Commit**

```
feat: add SpaceWithType, filter vault spaces from visible list
```

---

### Task 6: Use `visibleSpaces` in the spaces UI component

**Files:**
- Modify: `src/components/haex/system/settings/spaces.vue`

**Step 1: Use visibleSpaces instead of spaces**

In `src/components/haex/system/settings/spaces.vue`, change line 259:

```typescript
const { spaces } = storeToRefs(spacesStore)
```

To:

```typescript
const { visibleSpaces } = storeToRefs(spacesStore)
```

**Step 2: Update template references**

Change line 54:
```html
v-else-if="spaces.length"
```
To:
```html
v-else-if="visibleSpaces.length"
```

Change line 58 (the v-for):
```html
v-for="space in spaces"
```
To:
```html
v-for="space in visibleSpaces"
```

**Step 3: Commit**

```
feat: hide vault space from spaces list
```

---

### Task 7: Block invite and delete on vault spaces (defense in depth)

Even though vault spaces are hidden from the list, add store-level guards to prevent accidental invites/deletes via code paths.

**Files:**
- Modify: `src/stores/spaces.ts`

**Step 1: Add guard to `inviteMemberAsync`**

At the start of `inviteMemberAsync` (after the function signature), add:

```typescript
const space = spaces.value.find(s => s.id === spaceId)
if (space?.type === 'vault') throw new Error('Cannot invite members to vault space')
```

**Step 2: Add guard to `createInviteTokenAsync`**

At the start of `createInviteTokenAsync`, add:

```typescript
const space = spaces.value.find(s => s.id === spaceId)
if (space?.type === 'vault') throw new Error('Cannot create invite tokens for vault space')
```

**Step 3: Add guard to `deleteSpaceAsync`**

At the start of `deleteSpaceAsync`, add:

```typescript
const space = spaces.value.find(s => s.id === spaceId)
if (space?.type === 'vault') throw new Error('Cannot delete vault space')
```

**Step 4: Commit**

```
feat: add store-level guards against vault space invite/delete
```

---

### Task 8: Make space type immutable in edit dialog

The edit dialog currently allows changing the server URL (which could effectively change the space type). Prevent type changes.

**Files:**
- Modify: `src/components/haex/system/settings/spaces.vue`
- Modify: `src/stores/spaces.ts`

**Step 1: Disable server migration for local spaces**

In `src/components/haex/system/settings/spaces.vue`, the edit dialog shows a server URL selector. Add a condition to disable it for local spaces.

In the template, find the `USelectMenu` for `editForm.serverUrl` (around line 177) and add a `:disabled` prop:

```html
<USelectMenu
  v-model="editForm.serverUrl"
  :items="editServerOptions"
  :placeholder="t('edit.serverLabel')"
  :disabled="editingSpaceIsLocal"
  class="flex-1"
/>
```

In the script, add the computed:

```typescript
const editingSpaceIsLocal = computed(() => {
  const space = spaces.value.find(s => s.id === editingSpace.value?.id)
  return space?.type === 'local'
})
```

Note: This uses `spaces` (the full list from the store, not `visibleSpaces`) since we need to check all spaces including vault. Import or destructure `spaces` from the store if not already done:

```typescript
const { visibleSpaces, spaces } = storeToRefs(spacesStore)
```

**Step 2: Add guard in `migrateSpaceServerAsync`**

In `src/stores/spaces.ts`, at the start of `migrateSpaceServerAsync`, add:

```typescript
const spaceEntry = spaces.value.find(s => s.id === spaceId)
if (spaceEntry?.type === 'local') throw new Error('Cannot change server for local spaces')
if (spaceEntry?.type === 'vault') throw new Error('Cannot change server for vault space')
```

**Step 3: Commit**

```
feat: make space type immutable — block server migration for local/vault spaces
```

---

### Task 9: Build and verify

**Step 1: Run type check**

Run: `cd /home/haex/Projekte/haex-vault && pnpm typecheck`

Expected: No type errors. Fix any that arise from the `SpaceWithType` changes.

**Step 2: Run build**

Run: `cd /home/haex/Projekte/haex-vault && pnpm build`

Expected: Build succeeds.

**Step 3: Commit if any fixes were needed**

```
fix: resolve type errors from SpaceWithType migration
```

---

## Summary of Changes

| File | Change |
|------|--------|
| `src/database/tableNames.json` | Add `leaderPriority` column + 4 new `_no_sync` tables |
| `src/database/schemas/haex.ts` | Add `leaderPriority` to `haexSpaceDevices` |
| `src/database/schemas/localDelivery.ts` | NEW: 4 buffer tables for local delivery service |
| `src/database/schemas/index.ts` | Export new schema |
| `src/config/vault-settings.ts` | 5 new housekeeping settings keys |
| `src/stores/spaces.ts` | `SpaceWithType`, `visibleSpaces`, vault guards, type immutability |
| `src/components/haex/system/settings/spaces.vue` | Use `visibleSpaces`, disable server edit for local |
| `src-tauri/database/migrations/` | Auto-generated by `pnpm drizzle:generate` |
