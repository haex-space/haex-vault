# Spaces as Unified Anchor — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make `spaces` the single anchor table for all sync partitions (vaults + shared spaces), with real FK constraints and clean CASCADE deletes.

**Architecture:** Every vault gets a `spaces` row with `type='vault'`. `sync_changes.space_id` is UUID with FK to `spaces.id`. One partition trigger on `spaces`, one naming scheme, one cascade path. Fresh setup — no migrations, no legacy.

**Tech Stack:** PostgreSQL (Drizzle ORM), Hono (TypeScript), Vue 3 / Nuxt (client)

**Repos:**
- Server: `/home/haex/Projekte/haex-sync-server`
- Client: `/home/haex/Projekte/haex-vault`
- SDK: `/home/haex/Projekte/haex-vault-sdk` (no changes expected)

---

## FK & Cascade Design

```
auth.users (id)
  └── spaces (owner_id → auth.users.id ON DELETE CASCADE)
        ├── vault_keys  (space_id UUID → spaces.id ON DELETE CASCADE)
        ├── sync_changes (space_id UUID → spaces.id ON DELETE CASCADE)  ← real FK!
        ├── space_members (space_id → spaces.id ON DELETE CASCADE)
        └── space_key_grants (space_id → spaces.id ON DELETE CASCADE)
```

User delete → spaces delete → everything gone. One cascade path.

---

## Task 1: Schema — `spaces` gets `type`, `sync_changes.vault_id` → `space_id` UUID

**Files:**
- Modify: `haex-sync-server/src/db/schema.ts`

**Step 1: Update `spaces` table**

```typescript
export const spaces = pgTable("spaces", {
  id: uuid("id").primaryKey().defaultRandom(),
  type: text("type").notNull().default("shared"), // 'vault' | 'shared'
  ownerId: uuid("owner_id")
    .notNull()
    .references(() => authUsers.id, { onDelete: "cascade" }),
  encryptedName: text("encrypted_name"),    // nullable for type='vault'
  nameNonce: text("name_nonce"),            // nullable for type='vault'
  currentKeyGeneration: integer("current_key_generation").notNull().default(1),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow(),
});
```

**Step 2: Update `vault_keys` table — replace `vault_id` TEXT with `space_id` UUID FK**

```typescript
export const vaultKeys = pgTable(
  "vault_keys",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => authUsers.id, { onDelete: "cascade" }),
    spaceId: uuid("space_id")
      .notNull()
      .references(() => spaces.id, { onDelete: "cascade" }),
    encryptedVaultKey: text("encrypted_vault_key").notNull(),
    encryptedVaultName: text("encrypted_vault_name").notNull(),
    vaultKeySalt: text("vault_key_salt").notNull(),
    ephemeralPublicKey: text("ephemeral_public_key").notNull(),
    vaultKeyNonce: text("vault_key_nonce").notNull(),
    vaultNameNonce: text("vault_name_nonce").notNull(),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => [
    uniqueIndex("vault_keys_user_space_idx").on(table.userId, table.spaceId),
    index("vault_keys_user_idx").on(table.userId),
  ]
);
```

`vault_id` TEXT is gone. `space_id` UUID FK replaces it.

**Step 3: Update `sync_changes` table — `vault_id` TEXT → `space_id` UUID FK**

```typescript
export const syncChanges = pgTable(
  "sync_changes",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => authUsers.id, { onDelete: "cascade" }),
    spaceId: uuid("space_id")
      .notNull()
      .references(() => spaces.id, { onDelete: "cascade" }),

    tableName: text("table_name").notNull(),
    rowPks: text("row_pks").notNull(),
    columnName: text("column_name"),
    hlcTimestamp: text("hlc_timestamp").notNull(),
    deviceId: text("device_id"),

    encryptedValue: text("encrypted_value"),
    nonce: text("nonce"),

    signature: text("signature"),
    signedBy: text("signed_by"),
    recordOwner: text("record_owner"),
    collaborative: boolean("collaborative").default(false),

    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => [
    uniqueIndex("sync_changes_unique_cell").on(
      table.spaceId,
      table.tableName,
      table.rowPks,
      table.columnName
    ),
    index("sync_changes_user_space_idx").on(table.userId, table.spaceId),
    index("sync_changes_hlc_idx").on(table.hlcTimestamp),
    index("sync_changes_updated_idx").on(table.updatedAt),
    index("sync_changes_device_idx").on(table.deviceId),
  ]
);
```

**Step 4: Update type exports**

Rename `VaultKey` fields, update `SyncChange` type — follows from schema changes automatically via Drizzle inference.

**Step 5: Generate schema**

```bash
cd /home/haex/Projekte/haex-sync-server && npm run db:generate
```

**Step 6: Commit**

```bash
git add src/db/schema.ts drizzle/
git commit -m "feat: spaces as unified anchor, space_id UUID FK on sync_changes and vault_keys"
```

---

## Task 2: Rewrite partitioning.sql

**Files:**
- Rewrite: `haex-sync-server/drizzle/partitioning.sql`

Since `sync_changes.space_id` is now UUID, the partitioning uses UUID as partition key. One trigger on `spaces`, one naming scheme.

**Key changes:**
- Partition key: `space_id` UUID (was `vault_id` TEXT)
- Primary key: `(id, space_id)` (PK must include partition key)
- Partition naming: `sync_changes_<uuid_underscored>` for everything
- Single `create_sync_partition()` on `spaces` INSERT
- Single `drop_sync_partition()` on `spaces` DELETE
- No more triggers on `vault_keys`
- RLS: vault partitions use `user_id` check, shared partitions use `space_members` check
- `can_access_sync_channel()` uses `spaces.type` instead of checking two tables

**Step 1: Rewrite the file**

**Step 2: Apply**

```bash
cd /home/haex/Projekte/haex-sync-server && npm run db:push
```

**Step 3: Commit**

```bash
git add drizzle/partitioning.sql
git commit -m "feat: unified partition triggers on spaces, UUID partition key"
```

---

## Task 3: Update routes — `vaultId` → `spaceId`

**Files:**
- Modify: `haex-sync-server/src/routes/sync.ts`
- Modify: `haex-sync-server/src/routes/sync.vaults.ts`
- Modify: `haex-sync-server/src/routes/sync.helpers.ts`
- Modify: `haex-sync-server/src/routes/sync.schemas.ts`

**Step 1: sync.helpers.ts — replace `isSpacePartition` with `getSpaceType`**

```typescript
export async function getSpaceType(spaceId: string): Promise<'vault' | 'shared' | null> {
  const result = await db.select({ type: spaces.type })
    .from(spaces)
    .where(eq(spaces.id, spaceId))
    .limit(1)
  return (result[0]?.type as 'vault' | 'shared') ?? null
}
```

Delete `isSpacePartition`.

**Step 2: sync.schemas.ts — rename `vaultId` → `spaceId` in Zod schemas**

In `pushChangesSchema`, `pullChangesSchema`, `pullColumnsSchema`: rename the field.

**Step 3: sync.ts — update push/pull/pull-columns routes**

- Request field: `vaultId` → `spaceId`
- DB queries: `syncChanges.vaultId` → `syncChanges.spaceId`
- `isSpacePartition(vaultId)` → `getSpaceType(spaceId)`
- `isSpaceSync` = `spaceType === 'shared'`
- Vault ownership check: `spaces.ownerId` instead of `vault_keys` lookup
- ON CONFLICT target: `syncChanges.spaceId` instead of `syncChanges.vaultId`

**Step 4: sync.vaults.ts — update vault routes**

- `POST /vault-key`: accept `spaceId` (or keep `vaultId` in API and map internally), create `spaces` entry in transaction, then insert `vault_keys` with `spaceId`
- `GET /vault-key/:spaceId`: query by `spaceId`
- `PATCH /vault-key/:spaceId`: query by `spaceId`
- `DELETE /vault/:spaceId`: delete from `spaces` (CASCADE handles vault_keys + partition)
- `DELETE /vaults`: delete from `spaces` where `type='vault'`
- `POST /partitions/create`: insert into `spaces` (trigger creates partition)
- `GET /vaults`: query `spaces` where `type='vault'` joined with `vault_keys`

**Step 5: Commit**

```bash
git add src/routes/
git commit -m "feat: rename vaultId to spaceId across all routes"
```

---

## Task 4: Update client — `vaultId` → `spaceId` in sync engine

**Files:**
- Modify: `haex-vault/src/stores/sync/engine/vaultKey.ts` — cache key: spaceId
- Modify: `haex-vault/src/stores/sync/engine/changes.ts` — push/pull params
- Modify: `haex-vault/src/stores/sync/engine/types.ts` — type definitions
- Modify: `haex-vault/src/stores/sync/engine/index.ts` — ensureSyncKeyAsync
- Modify: `haex-vault/src/stores/sync/orchestrator/push.ts` — push params
- Modify: `haex-vault/src/stores/sync/orchestrator/pull.ts` — pull params
- Modify: `haex-vault/src/stores/sync/orchestrator/index.ts` — backend init
- Modify: `haex-vault/src/database/schemas/haex.ts` — `haex_sync_backends.vaultId` → `spaceId`
- Modify: `haex-vault/src/composables/useCreateSyncConnection.ts` — connection setup
- Modify: `haex-vault/src/stores/vault/index.ts` — vault ID as space ID

Since we're doing a clean break, rename everywhere consistently. The API parameter sent to the server changes from `vaultId` to `spaceId`.

**Step 1: Update schema** — `haex_sync_backends.vaultId` → `spaceId`

**Step 2: Update sync engine** — all references to `vaultId` become `spaceId`

**Step 3: Update composables** — connection setup uses `spaceId`

**Step 4: Commit**

```bash
git add src/
git commit -m "feat: rename vaultId to spaceId in sync engine"
```

---

## Task 5: Test cascade paths

**Manual testing:**

1. Upload vault key → verify `spaces` entry (type='vault') + partition created + real FK exists
2. Push/Pull changes → data integrity
3. DELETE vault → spaces + vault_keys + sync_changes partition all gone
4. Create shared space → type='shared' + partition + push/pull works
5. DELETE shared space → partition dropped
6. DELETE user → everything cascades

---

## Summary

| Area | Change |
|------|--------|
| `schema.ts` | `spaces.type`, nullable name, CASCADE on all FKs. `sync_changes.space_id` UUID FK. `vault_keys.space_id` UUID FK. `vault_id` TEXT gone. |
| `partitioning.sql` | Rewrite. UUID partition key. Single trigger on `spaces`. |
| Routes | `vaultId` → `spaceId` everywhere. `isSpacePartition` → `getSpaceType`. Vault CRUD goes through `spaces`. |
| Client | `vaultId` → `spaceId` in sync engine, schemas, composables. |
| FK | `sync_changes.space_id → spaces.id ON DELETE CASCADE` — **real FK, problem solved** |
