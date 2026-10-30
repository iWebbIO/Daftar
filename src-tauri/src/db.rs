//! Local database: opening it, keeping it safe, and the plumbing every module
//! shares (timestamps, audit rows, settings).

use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

pub type Res<T> = Result<T, String>;

pub const SCHEMA: &str = include_str!("schema.sql");
const SCHEMA_VERSION: i64 = 1;

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn today() -> i64 {
    crate::jalali::today().jdn()
}

/// A short Persian sentence for the shopkeeper, and the technical detail on
/// stderr where it helps nobody in the shop but everything in a bug report.
///
/// Every failure that can reach a screen goes through here or through a literal
/// Persian sentence: a SQLite message in front of a customer is the one thing
/// this app must never do.
pub fn oops(what: &str, detail: impl std::fmt::Display) -> String {
    eprintln!("{what}: {detail}");
    what.to_string()
}

/// Opens the database, applying the schema and the durability settings.
///
/// WAL with `synchronous=NORMAL` is the right trade for a shop phone: a save
/// survives the app being killed or the battery dying, and it does not fsync on
/// every single statement.
pub fn open(path: &Path) -> Res<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| oops("پوشه داده ساخته نشد", e))?;
    }
    let conn = Connection::open(path).map_err(|e| oops("دفتر باز نشد", e))?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| oops("دفتر آماده نشد", e))?;
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(|e| oops("دفتر آماده نشد", e))?;
    conn.pragma_update(None, "foreign_keys", true)
        .map_err(|e| oops("دفتر آماده نشد", e))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| oops("دفتر آماده نشد", e))?;
    conn.execute_batch(SCHEMA)
        .map_err(|e| oops("دفتر آماده نشد", e))?;
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap_or(0);
    if version < SCHEMA_VERSION {
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(|e| oops("دفتر آماده نشد", e))?;
    }
    Ok(conn)
}

/// Reads a setting, falling back to the given default.
pub fn get_setting(conn: &Connection, key: &str, default: &str) -> String {
    conn.query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .ok()
        .flatten()
        .unwrap_or_else(|| default.to_string())
}

pub fn put_setting(conn: &Connection, key: &str, value: &str) -> Res<()> {
    conn.execute(
        "INSERT INTO setting(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )
    .map_err(|e| oops("ذخیره نشد", e))?;
    Ok(())
}

/// Everything that changes a document, a payment or a balance goes through here
/// first. The log is the app's version of a crossed-out line: nothing is lost.
pub fn audit(conn: &Connection, entity: &str, entity_id: i64, action: &str, payload: &str) -> Res<()> {
    conn.execute(
        "INSERT INTO audit(entity, entity_id, action, payload, at) VALUES(?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![entity, entity_id, action, payload, now_ms()],
    )
    .map_err(|e| oops("ثبت نشد", e))?;
    Ok(())
}

#[cfg(test)]
pub fn memory() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory db");
    conn.pragma_update(None, "foreign_keys", true).ok();
    conn.execute_batch(SCHEMA).expect("schema");
    conn
}
