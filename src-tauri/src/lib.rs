//! دفتر — the whole app in one Rust crate.
//!
//! The interface is a thin view over this: it sends `{ op, args }` and gets JSON
//! back. All the arithmetic, all the Jalali dates and every write live here, so
//! there is exactly one place a total can be wrong and exactly one place tests
//! have to look.

mod backup;
mod db;
mod domain;
mod dto;
mod invoice;
mod jalali;
mod money;
mod telegram;

use db::Res;
use rusqlite::Connection;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use tauri::Manager;

pub struct Dirs {
    /// daftar.db and its WAL files.
    pub data: PathBuf,
    /// Generated invoices, statements and backup zips waiting to be sent.
    pub files: PathBuf,
}

pub struct AppState {
    pub db: Mutex<Connection>,
    pub dirs: Dirs,
}

impl AppState {
    pub fn conn(&self) -> Res<MutexGuard<'_, Connection>> {
        self.db
            .lock()
            .map_err(|_| "دفتر موقتاً در دسترس نیست".to_string())
    }
}

/// What a command handler is given. It borrows the state rather than holding the
/// database lock, so a handler that talks to Telegram does not freeze the UI.
pub struct Ctx<'a> {
    pub state: &'a AppState,
}

impl Ctx<'_> {
    pub fn conn(&self) -> Res<MutexGuard<'_, Connection>> {
        self.state.conn()
    }
}

/// The single entry point. Every operation in the app is a name plus arguments,
/// which keeps the interface and the tests speaking the same language.
#[tauri::command]
fn call(op: String, args: Option<Value>, state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let ctx = Ctx { state: &state };
    let args = args.unwrap_or(Value::Object(Default::default()));
    route(&ctx, &op, &args)
}

pub fn route(ctx: &Ctx<'_>, op: &str, args: &Value) -> Res<Value> {
    if let Some(result) = domain::route(ctx, op, args) {
        return result;
    }
    if let Some(result) = invoice::handle(ctx, op, args) {
        return result;
    }
    if let Some(result) = telegram::handle(ctx, op, args) {
        return result;
    }
    if let Some(result) = backup::handle(ctx, op, args) {
        return result;
    }
    Err(format!("دستور ناشناخته: {op}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let base = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("پوشه داده پیدا نشد: {e}"))?;
            let dirs = Dirs {
                data: base.clone(),
                files: base.join("files"),
            };
            std::fs::create_dir_all(&dirs.files).ok();
            let conn = db::open(&dirs.data.join("daftar.db"))?;
            app.manage(AppState {
                db: Mutex::new(conn),
                dirs,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![call])
        .run(tauri::generate_context!())
        .expect("خطا در اجرای برنامه");
}
