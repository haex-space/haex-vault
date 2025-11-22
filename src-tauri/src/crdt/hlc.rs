// src-tauri/src/crdt/hlc.rs

use crate::table_names::TABLE_CRDT_CONFIGS;
use rusqlite::{params, Connection, Transaction};
use serde_json::json;
use std::{
    fmt::Debug,
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use thiserror::Error;
use uhlc::{HLCBuilder, Timestamp, HLC, ID};
use uuid::Uuid;

const HLC_NODE_ID_TYPE: &str = "hlc_node_id";
const HLC_TIMESTAMP_TYPE: &str = "hlc_timestamp";

#[derive(Error, Debug)]
pub enum HlcError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Failed to parse persisted HLC timestamp: {0}")]
    ParseTimestamp(String),
    #[error("Failed to parse persisted HLC state: {0}")]
    Parse(String),
    #[error("Failed to parse HLC Node ID: {0}")]
    ParseNodeId(String),
    #[error("HLC mutex was poisoned")]
    MutexPoisoned,
    #[error("Failed to create node ID: {0}")]
    CreateNodeId(#[from] uhlc::SizeError),
    #[error("No database connection available")]
    NoConnection,
    #[error("HLC service not initialized")]
    NotInitialized,
    #[error("Hex decode error: {0}")]
    HexDecode(String),
    #[error("UTF-8 conversion error: {0}")]
    Utf8Error(String),
    #[error("Failed to access device store: {0}")]
    DeviceStore(String),
}

impl From<tauri_plugin_store::Error> for HlcError {
    fn from(error: tauri_plugin_store::Error) -> Self {
        HlcError::DeviceStore(error.to_string())
    }
}

/// A thread-safe, persistent HLC service.
#[derive(Clone)]
pub struct HlcService {
    hlc: Arc<Mutex<Option<HLC>>>,
}

impl HlcService {
    /// Creates a new HLC service. The HLC will be initialized on first database access.
    pub fn new() -> Self {
        HlcService {
            hlc: Arc::new(Mutex::new(None)),
        }
    }

    /// Factory-Funktion: Erstellt und initialisiert einen neuen HLC-Service aus einer bestehenden DB-Verbindung.
    /// Dies ist die bevorzugte Methode zur Instanziierung.
    pub fn try_initialize(conn: &Connection, app_handle: &AppHandle) -> Result<Self, HlcError> {
        // 1. Hole oder erstelle eine persistente Node-ID
        let node_id_str = Self::get_or_create_device_id(app_handle)?;

        // Parse den String in ein Uuid-Objekt.
        let uuid = Uuid::parse_str(&node_id_str).map_err(|e| {
            HlcError::ParseNodeId(format!(
                "Stored device ID is not a valid UUID: {node_id_str}. Error: {e}"
            ))
        })?;

        // Hol dir die rohen 16 Bytes und erstelle daraus die uhlc::ID.
        // Das `*` dereferenziert den `&[u8; 16]` zu `[u8; 16]`, was `try_from` erwartet.
        let node_id = ID::try_from(*uuid.as_bytes()).map_err(|e| {
            HlcError::ParseNodeId(format!("Invalid node ID format from device store: {e:?}"))
        })?;

        // 2. Erstelle eine HLC-Instanz mit stabiler Identität
        let hlc = HLCBuilder::new()
            .with_id(node_id)
            .with_max_delta(Duration::from_secs(1))
            .build();

        // 3. Lade und wende den letzten persistenten Zeitstempel an
        if let Some(last_timestamp) = Self::load_last_timestamp(conn)? {
            hlc.update_with_timestamp(&last_timestamp).map_err(|e| {
                HlcError::Parse(format!(
                    "Failed to update HLC with persisted timestamp: {e:?}"
                ))
            })?;
        }

        Ok(HlcService {
            hlc: Arc::new(Mutex::new(Some(hlc))),
        })
    }

    /// Holt die Geräte-ID aus dem Tauri Store oder erstellt eine neue, wenn keine existiert.
    fn get_or_create_device_id(app_handle: &AppHandle) -> Result<String, HlcError> {
        let store_path = PathBuf::from("instance.json");
        let store = app_handle
            .store(store_path)
            .map_err(|e| HlcError::DeviceStore(e.to_string()))?;

        let id_exists = match store.get("id") {
            // Fall 1: Der Schlüssel "id" existiert UND sein Wert ist ein String.
            Some(value) => {
                if let Some(s) = value.as_str() {
                    // Das ist unser Erfolgsfall. Wir haben einen &str und können
                    // eine Kopie davon zurückgeben.
                    println!("Gefundene und validierte Geräte-ID: {s}");
                    if Uuid::parse_str(s).is_ok() {
                        // Erfolgsfall: Der Wert ist ein String UND eine gültige UUID.
                        // Wir können die Funktion direkt mit dem Wert verlassen.
                        return Ok(s.to_string());
                    }
                }
                // Der Wert existiert, ist aber kein String (z.B. eine Zahl).
                // Wir behandeln das, als gäbe es keine ID.
                false
            }
            // Fall 2: Der Schlüssel "id" existiert nicht.
            None => false,
        };

        // Wenn wir hier ankommen, bedeutet das, `id_exists` ist `false`.
        // Entweder weil der Schlüssel fehlte oder weil der Wert kein String war.
        // Also erstellen wir eine neue ID.
        if !id_exists {
            let new_id = Uuid::new_v4().to_string();

            store.set("id".to_string(), json!(new_id.clone()));

            store.save()?;

            return Ok(new_id);
        }

        // Dieser Teil des Codes sollte nie erreicht werden, aber der Compiler
        // braucht einen finalen return-Wert. Wir können hier einen Fehler werfen.
        Err(HlcError::DeviceStore(
            "Unreachable code: Failed to determine device ID".to_string(),
        ))
    }

    /// Generiert einen neuen Zeitstempel und persistiert den neuen Zustand des HLC sofort.
    /// Muss innerhalb einer bestehenden Datenbanktransaktion aufgerufen werden.
    pub fn new_timestamp_and_persist<'tx>(
        &self,
        tx: &Transaction<'tx>,
    ) -> Result<Timestamp, HlcError> {
        let mut hlc_guard = self.hlc.lock().map_err(|_| HlcError::MutexPoisoned)?;
        let hlc = hlc_guard.as_mut().ok_or(HlcError::NotInitialized)?;

        let new_timestamp = hlc.new_timestamp();
        Self::persist_timestamp(tx, &new_timestamp)?;

        Ok(new_timestamp)
    }

    /// Erstellt einen neuen Zeitstempel, ohne ihn zu persistieren (z.B. für Leseoperationen).
    pub fn new_timestamp(&self) -> Result<Timestamp, HlcError> {
        let mut hlc_guard = self.hlc.lock().map_err(|_| HlcError::MutexPoisoned)?;
        let hlc = hlc_guard.as_mut().ok_or(HlcError::NotInitialized)?;

        Ok(hlc.new_timestamp())
    }

    /// Aktualisiert den HLC mit einem externen Zeitstempel (für die Synchronisation).
    pub fn update_with_timestamp(&self, timestamp: &Timestamp) -> Result<(), HlcError> {
        let mut hlc_guard = self.hlc.lock().map_err(|_| HlcError::MutexPoisoned)?;
        let hlc = hlc_guard.as_mut().ok_or(HlcError::NotInitialized)?;

        hlc.update_with_timestamp(timestamp)
            .map_err(|e| HlcError::Parse(format!("Failed to update HLC: {e:?}")))
    }

    /// Lädt den letzten persistierten Zeitstempel aus der Datenbank.
    fn load_last_timestamp(conn: &Connection) -> Result<Option<Timestamp>, HlcError> {
        let query = format!("SELECT value FROM {TABLE_CRDT_CONFIGS} WHERE key = ?1 AND type = 'hlc'");

        match conn.query_row(&query, params![HLC_TIMESTAMP_TYPE], |row| {
            row.get::<_, String>(0)
        }) {
            Ok(state_str) => {
                let timestamp = Timestamp::from_str(&state_str).map_err(|e| {
                    HlcError::ParseTimestamp(format!("Invalid timestamp format: {e:?}"))
                })?;
                Ok(Some(timestamp))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(HlcError::Database(e)),
        }
    }

    /// Persistiert einen Zeitstempel in der Datenbank innerhalb einer Transaktion.
    fn persist_timestamp(tx: &Transaction, timestamp: &Timestamp) -> Result<(), HlcError> {
        let timestamp_str = timestamp.to_string();
        tx.execute(
            &format!(
                "INSERT INTO {TABLE_CRDT_CONFIGS} (key, type, value) VALUES (?1, 'hlc', ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value"
            ),
            params![HLC_TIMESTAMP_TYPE, timestamp_str],
        )?;
        Ok(())
    }
}

impl Default for HlcService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::str::FromStr;

    #[test]
    fn test_timestamp_format() {
        // Verify that uhlc 0.8.2 uses the "time/node_id_hex" format
        let node_id = ID::try_from([1u8; 16]).unwrap();
        let hlc = HLCBuilder::new()
            .with_id(node_id)
            .with_max_delta(Duration::from_secs(1))
            .build();

        let timestamp = hlc.new_timestamp();
        let formatted = timestamp.to_string();

        println!("HLC Timestamp format: {}", formatted);
        println!("Length: {}", formatted.len());

        // Verify format: "number/hex_string"
        assert!(formatted.contains('/'), "Timestamp should contain '/'");
        let parts: Vec<&str> = formatted.split('/').collect();
        assert_eq!(parts.len(), 2, "Timestamp should have exactly 2 parts");

        // First part should be a valid u64
        let time_part = parts[0].parse::<u64>();
        assert!(time_part.is_ok(), "Time part should be a valid u64");

        // Second part should be hex representation of node_id
        // Note: Leading zeros may be omitted in hex representation
        assert!(
            parts[1].len() <= 32,
            "Node ID hex should be at most 32 characters (16 bytes)"
        );
        assert!(
            !parts[1].is_empty(),
            "Node ID hex should not be empty"
        );
    }

    #[test]
    fn test_timestamp_parsing() {
        // Verify that we can parse timestamps back from string
        let node_id = ID::try_from([2u8; 16]).unwrap();
        let hlc = HLCBuilder::new()
            .with_id(node_id)
            .with_max_delta(Duration::from_secs(1))
            .build();

        let original = hlc.new_timestamp();
        let formatted = original.to_string();

        // Parse it back
        let parsed = Timestamp::from_str(&formatted).expect("Should parse timestamp");

        // Verify they're equal
        assert_eq!(original, parsed, "Parsed timestamp should equal original");
    }

    #[test]
    fn test_timestamp_time_extraction() {
        // Verify that we can extract the time component correctly
        let node_id = ID::try_from([3u8; 16]).unwrap();
        let hlc = HLCBuilder::new()
            .with_id(node_id)
            .with_max_delta(Duration::from_secs(1))
            .build();

        let timestamp = hlc.new_timestamp();
        let formatted = timestamp.to_string();

        // Extract time via API
        let time_via_api = timestamp.get_time().as_u64();

        // Extract time via string parsing (like in cleanup.rs)
        let time_via_string: u64 = formatted
            .split('/')
            .next()
            .unwrap()
            .parse()
            .expect("Should parse time component");

        assert_eq!(
            time_via_api, time_via_string,
            "Time extraction via API and string should match"
        );
    }

    #[test]
    fn test_timestamp_ordering() {
        // Verify that timestamps are monotonically increasing
        let node_id = ID::try_from([4u8; 16]).unwrap();
        let hlc = HLCBuilder::new()
            .with_id(node_id)
            .with_max_delta(Duration::from_secs(1))
            .build();

        let ts1 = hlc.new_timestamp();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let ts2 = hlc.new_timestamp();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let ts3 = hlc.new_timestamp();

        // Timestamps should be ordered
        assert!(ts1 < ts2, "ts1 should be less than ts2");
        assert!(ts2 < ts3, "ts2 should be less than ts3");

        // Time components should also be ordered
        assert!(
            ts1.get_time().as_u64() <= ts2.get_time().as_u64(),
            "Time components should be non-decreasing"
        );
        assert!(
            ts2.get_time().as_u64() <= ts3.get_time().as_u64(),
            "Time components should be non-decreasing"
        );
    }

    #[test]
    fn test_hlc_persistence() {
        // Verify that HLC timestamps can be persisted and loaded
        let mut conn = Connection::open_in_memory().expect("Should create in-memory DB");

        // Create the config table
        conn.execute(
            &format!(
                "CREATE TABLE {TABLE_CRDT_CONFIGS} (key TEXT PRIMARY KEY, type TEXT NOT NULL, value TEXT NOT NULL)"
            ),
            [],
        )
        .expect("Should create table");

        let node_id = ID::try_from([5u8; 16]).unwrap();
        let hlc = HLCBuilder::new()
            .with_id(node_id)
            .with_max_delta(Duration::from_secs(1))
            .build();

        let original_timestamp = hlc.new_timestamp();

        // Persist it
        {
            let tx = conn.transaction().expect("Should start transaction");
            HlcService::persist_timestamp(&tx, &original_timestamp)
                .expect("Should persist timestamp");
            tx.commit().expect("Should commit");
        }

        // Load it back
        let loaded_timestamp =
            HlcService::load_last_timestamp(&conn).expect("Should load timestamp");

        assert!(loaded_timestamp.is_some(), "Should have loaded a timestamp");
        assert_eq!(
            loaded_timestamp.unwrap(),
            original_timestamp,
            "Loaded timestamp should match original"
        );
    }

    #[test]
    fn test_timestamp_difference_calculation() {
        // Verify that we can correctly calculate time differences
        let node_id = ID::try_from([6u8; 16]).unwrap();
        let hlc = HLCBuilder::new()
            .with_id(node_id)
            .with_max_delta(Duration::from_secs(1))
            .build();

        let ts1 = hlc.new_timestamp();
        let time1 = ts1.get_time().as_u64();

        // Simulate aging by a known amount (e.g., 1 day in nanoseconds)
        let one_day_ns: u64 = 24 * 60 * 60 * 1_000_000_000;
        let cutoff_time = time1.saturating_sub(one_day_ns);

        // Verify the calculation
        assert!(
            cutoff_time < time1,
            "Cutoff should be less than current time"
        );
        assert_eq!(
            time1 - cutoff_time,
            one_day_ns,
            "Difference should be exactly one day"
        );
    }

    #[test]
    fn test_ntp64_nanosecond_precision() {
        // Verify that NTP64 timestamps have nanosecond precision
        let node_id = ID::try_from([7u8; 16]).unwrap();
        let hlc = HLCBuilder::new()
            .with_id(node_id)
            .with_max_delta(Duration::from_secs(1))
            .build();

        let ts = hlc.new_timestamp();
        let ntp64_value = ts.get_time().as_u64();

        // NTP64 should be a large number (nanoseconds since 1900)
        // As of 2024, this should be > 10^18
        assert!(
            ntp64_value > 1_000_000_000_000_000_000,
            "NTP64 value should be in nanoseconds range: {}",
            ntp64_value
        );

        println!("NTP64 value: {}", ntp64_value);
    }

    #[test]
    fn test_update_with_external_timestamp() {
        // Verify that HLC can be updated with external timestamps (for sync)
        let node_id1 = ID::try_from([8u8; 16]).unwrap();
        let node_id2 = ID::try_from([9u8; 16]).unwrap();

        let hlc1 = HLCBuilder::new()
            .with_id(node_id1)
            .with_max_delta(Duration::from_secs(1))
            .build();

        let hlc2 = HLCBuilder::new()
            .with_id(node_id2)
            .with_max_delta(Duration::from_secs(1))
            .build();

        let ts1 = hlc1.new_timestamp();

        // Simulate time passing
        std::thread::sleep(std::time::Duration::from_millis(10));

        let ts2_before = hlc2.new_timestamp();

        // Update hlc2 with ts1 (simulating receiving a remote timestamp)
        hlc2.update_with_timestamp(&ts1)
            .expect("Should update with external timestamp");

        let ts2_after = hlc2.new_timestamp();

        // ts2_after should be greater than both ts1 and ts2_before
        assert!(
            ts2_after > ts1,
            "Updated timestamp should be greater than external timestamp"
        );
        assert!(
            ts2_after > ts2_before,
            "Updated timestamp should be greater than previous timestamp"
        );
    }
}
