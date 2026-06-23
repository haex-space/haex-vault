// src-tauri/src/database/core/connection.rs

use crate::database::error::DatabaseError;
use crate::database::DbConnection;
use rusqlite::Connection;

pub fn with_connection<T, F>(connection: &DbConnection, f: F) -> Result<T, DatabaseError>
where
    F: FnOnce(&mut Connection) -> Result<T, DatabaseError>,
{
    let mut db_lock = connection
        .0
        .lock()
        .map_err(|e| DatabaseError::MutexPoisoned {
            reason: e.to_string(),
        })?;

    let conn = db_lock.as_mut().ok_or(DatabaseError::ConnectionError {
        reason: "Connection to vault failed".to_string(),
    })?;

    f(conn)
}
