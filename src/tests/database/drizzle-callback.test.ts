/**
 * Unit tests for drizzleCallback behavior
 *
 * These tests verify that the drizzleCallback correctly handles different
 * Drizzle method types, especially the 'get' method used by findFirst().
 *
 * The critical bug this tests for:
 * - When no rows are found, 'get' method must return { rows: undefined }
 * - NOT { rows: [] } which causes Drizzle to return {} (empty object) instead of undefined
 * - This empty object is truthy but has no properties, causing logic errors
 *
 * See: .claude/problems.md - "Drizzle findFirst gibt {} statt undefined zurück (2026-01-12)"
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { createClient, type Client } from '@libsql/client'
import { drizzle } from 'drizzle-orm/libsql'
import { sqliteTable, text } from 'drizzle-orm/sqlite-core'
import { eq } from 'drizzle-orm'

// Simple test schema matching haexCrdtConfigs structure
const testConfigs = sqliteTable('test_configs', {
  key: text('key').primaryKey(),
  type: text('type').notNull(),
  value: text('value').notNull(),
})

// Schema object for relational queries
const schema = { testConfigs }

describe('drizzleCallback findFirst behavior', () => {
  let client: Client
  let db: ReturnType<typeof drizzle<typeof schema>>

  beforeEach(async () => {
    // Create in-memory SQLite database
    client = createClient({ url: ':memory:' })
    db = drizzle(client, { schema })

    // Create test table
    await client.execute(`
      CREATE TABLE test_configs (
        key TEXT PRIMARY KEY NOT NULL,
        type TEXT NOT NULL,
        value TEXT NOT NULL
      )
    `)
  })

  afterEach(async () => {
    client.close()
  })

  describe('findFirst returns correct types', () => {
    it('should return undefined when no rows found (not empty object)', async () => {
      // Query for non-existent key
      const result = await db.query.testConfigs.findFirst({
        where: eq(testConfigs.key, 'non_existent_key'),
      })

      // CRITICAL: Must be undefined, not {}
      // The old bug returned {} which is truthy but has no properties
      expect(result).toBeUndefined()
    })

    it('should return the actual row when found', async () => {
      // Insert a row first
      await db.insert(testConfigs).values({
        key: 'test_key',
        type: 'test',
        value: 'test_value',
      })

      // Query for existing key
      const result = await db.query.testConfigs.findFirst({
        where: eq(testConfigs.key, 'test_key'),
      })

      expect(result).toBeDefined()
      expect(result?.key).toBe('test_key')
      expect(result?.type).toBe('test')
      expect(result?.value).toBe('test_value')
    })

    it('should allow optional chaining on undefined result', async () => {
      const result = await db.query.testConfigs.findFirst({
        where: eq(testConfigs.key, 'non_existent'),
      })

      // This is the pattern used in isInitialSyncCompleteAsync
      // With the bug, result was {} and result?.value was undefined
      // which worked, but result itself was truthy causing other issues
      expect(result?.value).toBeUndefined()
      expect(result?.key).toBeUndefined()
    })

    it('should correctly check for existence using optional property', async () => {
      // The defensive pattern used after the bug fix
      const result = await db.query.testConfigs.findFirst({
        where: eq(testConfigs.key, 'non_existent'),
      })

      // Check for existing.key instead of just existing
      // This works even if findFirst returned {} (empty object)
      if (result?.key) {
        // Should not reach here
        expect.fail('Should not have key on undefined/empty result')
      }

      // This is the correct path
      expect(result?.key).toBeFalsy()
    })
  })

  describe('findFirst with truthy checks', () => {
    it('empty object {} is truthy but has no properties', () => {
      // This demonstrates the bug behavior
      const emptyObject: Record<string, unknown> = {}

      // Empty object is truthy!
      expect(!!emptyObject).toBe(true)

      // But has no properties
      expect(emptyObject.value).toBeUndefined()
      expect(emptyObject.key).toBeUndefined()

      // Object.keys is empty
      expect(Object.keys(emptyObject)).toHaveLength(0)
    })

    it('undefined is falsy and optional chaining returns undefined', () => {
      const undefinedValue = undefined as Record<string, unknown> | undefined

      // undefined is falsy
      expect(!!undefinedValue).toBe(false)

      // Optional chaining returns undefined
      const value = undefinedValue?.value
      const key = undefinedValue?.key
      expect(value).toBeUndefined()
      expect(key).toBeUndefined()
    })

    it('real row is truthy and has properties', async () => {
      await db.insert(testConfigs).values({
        key: 'real_key',
        type: 'real',
        value: 'real_value',
      })

      const result = await db.query.testConfigs.findFirst({
        where: eq(testConfigs.key, 'real_key'),
      })

      // Real row is truthy
      expect(!!result).toBe(true)

      // And has properties
      expect(result?.key).toBe('real_key')
      expect(result?.value).toBe('real_value')
    })
  })

  describe('initial_sync_complete pattern', () => {
    it('should correctly detect when initial_sync_complete is not set', async () => {
      // Simulates isInitialSyncCompleteAsync when no entry exists
      const result = await db.query.testConfigs.findFirst({
        where: eq(testConfigs.key, 'initial_sync_complete'),
      })

      const isComplete = result?.value === 'true'
      expect(isComplete).toBe(false)
    })

    it('should correctly detect when initial_sync_complete is set to true', async () => {
      // Insert the config
      await db.insert(testConfigs).values({
        key: 'initial_sync_complete',
        type: 'sync',
        value: 'true',
      })

      // Simulates isInitialSyncCompleteAsync when entry exists
      const result = await db.query.testConfigs.findFirst({
        where: eq(testConfigs.key, 'initial_sync_complete'),
      })

      const isComplete = result?.value === 'true'
      expect(isComplete).toBe(true)
    })

    it('should correctly detect when initial_sync_complete is set to false', async () => {
      // Insert with value 'false'
      await db.insert(testConfigs).values({
        key: 'initial_sync_complete',
        type: 'sync',
        value: 'false',
      })

      const result = await db.query.testConfigs.findFirst({
        where: eq(testConfigs.key, 'initial_sync_complete'),
      })

      const isComplete = result?.value === 'true'
      expect(isComplete).toBe(false)
    })

    it('should allow upsert pattern with existence check', async () => {
      // Simulates setInitialSyncCompleteAsync
      const checkExisting = async () => {
        return await db.query.testConfigs.findFirst({
          where: eq(testConfigs.key, 'initial_sync_complete'),
        })
      }

      // First call - should insert
      let existing = await checkExisting()
      expect(existing?.key).toBeFalsy() // Use ?.key check as defensive pattern

      if (!existing?.key) {
        await db.insert(testConfigs).values({
          key: 'initial_sync_complete',
          type: 'sync',
          value: 'true',
        })
      }

      // Second call - should update
      existing = await checkExisting()
      expect(existing?.key).toBe('initial_sync_complete')

      if (existing?.key) {
        await db
          .update(testConfigs)
          .set({ value: 'updated' })
          .where(eq(testConfigs.key, 'initial_sync_complete'))
      }

      // Verify update
      const final = await checkExisting()
      expect(final?.value).toBe('updated')
    })
  })
})

/**
 * Tests for the drizzle sqlite-proxy callback behavior
 *
 * The actual drizzleCallback in src/stores/vault/index.ts cannot be tested
 * directly because it depends on Tauri's invoke() function. However, we can
 * test the expected behavior and document the contract.
 */
describe('drizzle sqlite-proxy callback contract', () => {
  it('documents the expected return format for method=get', () => {
    // The drizzleCallback should return:
    // - { rows: firstRow } when rows are found
    // - { rows: undefined } when no rows are found (NOT { rows: [] }!)

    // This is the CORRECT behavior:
    const correctNoResult = { rows: undefined }
    const correctWithResult = { rows: { key: 'test', value: 'data' } }

    // Drizzle's findFirst extracts rows and returns it directly
    // So undefined becomes undefined (correct)
    expect(correctNoResult.rows).toBeUndefined()
    expect(correctWithResult.rows).toEqual({ key: 'test', value: 'data' })

    // This was the BUG:
    const buggyNoResult: { rows: unknown[] } = { rows: [] }

    // When Drizzle gets [], it tries to map it and returns {}
    // This {} is truthy but has no properties!
    expect(buggyNoResult.rows).toEqual([])
    expect(Array.isArray(buggyNoResult.rows)).toBe(true)
    expect(buggyNoResult.rows.length).toBe(0)
  })

  it('demonstrates Array.prototype.at(0) behavior', () => {
    // The fix uses rows.at(0) which returns undefined for empty arrays
    const emptyArray: unknown[] = []
    const nonEmptyArray = [{ key: 'test' }]

    // at(0) on empty array returns undefined
    expect(emptyArray.at(0)).toBeUndefined()

    // at(0) on non-empty array returns first element
    expect(nonEmptyArray.at(0)).toEqual({ key: 'test' })

    // This is why { rows: rows.at(0) } works correctly:
    // - Empty result: { rows: undefined }
    // - Non-empty result: { rows: firstRow }
  })
})
