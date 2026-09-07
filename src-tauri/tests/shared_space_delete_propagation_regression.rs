//! Obsolete after the `_no_trigger` clean-sweep migration regeneration.
//!
//! Both tests in this file addressed the fresh-vault vs. retrofit split
//! between migrations 0012 and 0013 (ADR 0002 §6.5, "Runde-10" fix). The
//! clean sweep collapsed migrations 0000..=0020 into a single
//! `0000_ordinary_microchip.sql`, so there is no longer a "0012 stopping
//! point" for the retrofit path to reach, and no separate 0013 to apply
//! on top. The fresh-vault schema is now validated end-to-end by
//! `crdt::scanner::tests::whitelisted_tables_exist_in_the_migration_schema`,
//! which replays the consolidated migration and asserts every whitelisted
//! table exists — the same guarantee this file used to lock in.
