/**
 * Integration tests for Drizzle RETURNING clause functionality
 *
 * These tests verify that:
 * 1. SQLite RETURNING clause works correctly with drizzle-orm
 * 2. DEFAULT values (like CURRENT_TIMESTAMP) are returned
 * 3. The drizzle callback logic correctly routes queries
 *
 * Note: These tests use @libsql/client (in-memory SQLite) directly,
 * not the Tauri/Rust backend. They test the JavaScript/Drizzle layer.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { createClient, type Client } from '@libsql/client'
import { drizzle } from 'drizzle-orm/libsql'
import { sql } from 'drizzle-orm'
import { sqliteTable, text, integer } from 'drizzle-orm/sqlite-core'

// Simple test schema
const testTable = sqliteTable('test_items', {
  id: text('id').primaryKey(),
  name: text('name').notNull(),
  createdAt: text('created_at').default(sql`(CURRENT_TIMESTAMP)`),
  counter: integer('counter').notNull().default(0),
})

describe('Drizzle RETURNING clause', () => {
  let client: Client
  let db: ReturnType<typeof drizzle>

  beforeEach(async () => {
    // Create in-memory SQLite database
    client = createClient({ url: ':memory:' })
    db = drizzle(client)

    // Create test table
    await client.execute(`
      CREATE TABLE test_items (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL,
        created_at TEXT DEFAULT (CURRENT_TIMESTAMP),
        counter INTEGER NOT NULL DEFAULT 0
      )
    `)
  })

  afterEach(async () => {
    client.close()
  })

  it('should return inserted row with default values using RETURNING', async () => {
    const id = crypto.randomUUID()

    // INSERT with RETURNING
    const result = await db
      .insert(testTable)
      .values({ id, name: 'Test Item' })
      .returning()

    expect(result).toHaveLength(1)
    expect(result[0]).toBeDefined()
    expect(result[0]!.id).toBe(id)
    expect(result[0]!.name).toBe('Test Item')
    expect(result[0]!.counter).toBe(0) // Default value
    expect(result[0]!.createdAt).toBeDefined() // DB-generated timestamp
    expect(result[0]!.createdAt).toMatch(/^\d{4}-\d{2}-\d{2}/) // ISO date format
  })

  it('should return specific columns with RETURNING', async () => {
    const id = crypto.randomUUID()

    // INSERT with specific RETURNING columns
    const result = await db
      .insert(testTable)
      .values({ id, name: 'Specific Columns' })
      .returning({ id: testTable.id, createdAt: testTable.createdAt })

    expect(result).toHaveLength(1)
    expect(result[0]!.id).toBe(id)
    expect(result[0]!.createdAt).toBeDefined()
    // Should NOT have name or counter since we didn't request them
    expect('name' in result[0]!).toBe(false)
    expect('counter' in result[0]!).toBe(false)
  })

  it('should return updated row with RETURNING', async () => {
    const id = crypto.randomUUID()

    // First insert
    await db.insert(testTable).values({ id, name: 'Original' })

    // UPDATE with RETURNING
    const result = await db
      .update(testTable)
      .set({ name: 'Updated', counter: 42 })
      .where(sql`id = ${id}`)
      .returning()

    expect(result).toHaveLength(1)
    expect(result[0]!.name).toBe('Updated')
    expect(result[0]!.counter).toBe(42)
    expect(result[0]!.createdAt).toBeDefined() // Original timestamp preserved
  })

  it('should return deleted row with RETURNING', async () => {
    const id = crypto.randomUUID()

    // First insert
    await db.insert(testTable).values({ id, name: 'To Delete' })

    // DELETE with RETURNING
    const result = await db
      .delete(testTable)
      .where(sql`id = ${id}`)
      .returning()

    expect(result).toHaveLength(1)
    expect(result[0]!.id).toBe(id)
    expect(result[0]!.name).toBe('To Delete')

    // Verify it's actually deleted
    const remaining = await db.select().from(testTable)
    expect(remaining).toHaveLength(0)
  })

  it('should handle multiple inserts with RETURNING', async () => {
    const ids = [crypto.randomUUID(), crypto.randomUUID(), crypto.randomUUID()]

    const result = await db
      .insert(testTable)
      .values([
        { id: ids[0], name: 'First' },
        { id: ids[1], name: 'Second' },
        { id: ids[2], name: 'Third' },
      ])
      .returning()

    expect(result).toHaveLength(3)
    expect(result.map((r) => r.name)).toEqual(['First', 'Second', 'Third'])
    // All should have timestamps
    result.forEach((r) => {
      expect(r.createdAt).toBeDefined()
    })
  })
})

/**
 * Note: SQL statement type detection (SELECT vs INSERT/UPDATE/DELETE, RETURNING clause)
 * is now handled entirely in the Rust backend via AST parsing (sqlparser crate).
 *
 * The frontend drizzleCallback simply calls `sql_with_crdt` and the backend
 * correctly routes the query based on AST analysis - no string matching needed.
 *
 * See: src-tauri/src/database/mod.rs::sql_with_crdt
 * See: src-tauri/src/database/core.rs::statement_has_returning
 */
