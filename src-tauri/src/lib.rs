//! دفتر — the whole app in one Rust crate.
//!
//! The interface is a thin view over this: it sends `{ op, args }` and gets JSON
//! back. All the arithmetic, all the Jalali dates and every write live here, so
//! there is exactly one place a total can be wrong and exactly one place tests
//! have to look.

mod android_share;
mod backup;
mod db;
mod domain;
mod dto;
mod invoice;
mod jalali;
mod money;
mod share;
mod telegram;
mod update;

use db::Res;
use rusqlite::Connection;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use tauri::{Emitter, Manager};

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
///
/// This is `async` and hands the work to a blocking thread on purpose: sending
/// an invoice or waiting for the shopkeeper to press Start on his bot can take
/// half a minute, and none of that may stall the window.
#[tauri::command]
async fn call(app: tauri::AppHandle, op: String, args: Option<Value>) -> Result<Value, String> {
    let args = args.unwrap_or(Value::Object(Default::default()));
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let ctx = Ctx { state: &state };
        route(&ctx, &op, &args)
    })
    .await
    .map_err(|e| format!("اجرا نشد: {e}"))?
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
    if let Some(result) = share::handle(ctx, op, args) {
        return result;
    }
    if let Some(result) = update::handle(ctx, op, args) {
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
        .plugin(android_share::init())
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

            // The daily snapshot, and a check for a newer version: both on their
            // own thread, both quiet. The backup goes first on purpose — a check
            // against a blocked GitHub can sit for fifteen seconds, and a closed
            // app in that window would lose the day's copy.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let state = handle.state::<AppState>();
                let ctx = Ctx { state: &state };
                if backup::due(&ctx) {
                    let _ = route(&ctx, "backup.now", &Value::Object(Default::default()));
                }
                if let Some(info) = update::check(&ctx) {
                    let _ = handle.emit("daftar://update", info);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![call])
        .run(tauri::generate_context!())
        .expect("خطا در اجرای برنامه");
}
