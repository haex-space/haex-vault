import { integer, sqliteTable, text } from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'

/**
 * Persistent record of critical-failure events local to this machine
 * (mutex poisoning, audit-log write failures, schema drift). NOT
 * CRDT-synced — `_no_sync` suffix marks the table as local-only.
 *
 * Written by `crate::critical::CriticalNotificationSink` on the Rust side
 * via a SEPARATE SQLite connection so that a poisoned main DB mutex still
 * lets the sink record what happened.
 *
 * The Vue banner queries `acknowledged = 0` and shows the newest unacked
 * row. Acknowledged rows stay in the table as a forensic trail until the
 * configured retention cleanup removes them (analogous to haex_logs).
 *
 * See docs/plans/2026-06-13-critical-failure-pattern.md for the full
 * design and the three open-questions decisions (Q1: acknowledged rows
 * persist for retention; Q2: severity is a property of the code, not the
 * call; Q3: dedup on (code, location, acknowledged) with count++).
 */
export const haexCriticalNotificationsNoSync = sqliteTable(
  tableNames.haex.critical_notifications_no_sync.name,
  {
    id: text(tableNames.haex.critical_notifications_no_sync.columns.id).primaryKey(),
    /** Discriminator matching the Rust `CriticalFailureCode` enum. */
    code: text(tableNames.haex.critical_notifications_no_sync.columns.code).notNull(),
    /** Source location (e.g. `crdt::hlc::next_timestamp`) for forensics; NOT shown in the user-facing banner. */
    location: text(tableNames.haex.critical_notifications_no_sync.columns.location).notNull(),
    /** JSON object with dynamic substitution parameters for the localized message. */
    params: text(tableNames.haex.critical_notifications_no_sync.columns.params).notNull(),
    /** Number of times this (code, location, acknowledged) tuple has fired. UPSERT increments this. */
    count: integer(tableNames.haex.critical_notifications_no_sync.columns.count).notNull().default(1),
    /** RFC3339 timestamp of the earliest occurrence in the current row. */
    firstSeen: text(tableNames.haex.critical_notifications_no_sync.columns.firstSeen).notNull(),
    /** RFC3339 timestamp of the most recent occurrence; banner orders by this DESC. */
    lastSeen: text(tableNames.haex.critical_notifications_no_sync.columns.lastSeen).notNull(),
    /** 0 = unacked (drives banner), 1 = user dismissed (kept as forensic trail until retention cleanup). */
    acknowledged: integer(tableNames.haex.critical_notifications_no_sync.columns.acknowledged, { mode: 'boolean' }).notNull().default(false),
  },
)

export type InsertHaexCriticalNotification = typeof haexCriticalNotificationsNoSync.$inferInsert
export type SelectHaexCriticalNotification = typeof haexCriticalNotificationsNoSync.$inferSelect
