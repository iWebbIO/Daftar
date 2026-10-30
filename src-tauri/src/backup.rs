//! Daily snapshot of the ledger: a consistent copy of the database, zipped and
//! handed to the shopkeeper's own Telegram bot.
//!
//! Three promises, in this order:
//!
//! 1. The snapshot is made by SQLite itself (`VACUUM INTO`), never by copying a
//!    live file that has a WAL beside it — a copied WAL pair is a database that
//!    only looks whole.
//! 2. The zip is written to the phone *before* the network is touched, so a dead
//!    connection costs him nothing.
//! 3. Nothing here can fail in a way that stops the app from saving. The worst
//!    outcome is a Persian sentence saying the backup did not leave the phone.

use crate::db::{self, Res};
use crate::dto::BackupInfo;
use crate::telegram::client::{self, Transport, Https};
use crate::Ctx;
use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Epoch milliseconds of the last backup that actually reached Telegram. The
/// document lists also read this key, so it is only written on success.
pub const LAST_SENT: &str = "last_backup_at";
/// Epoch milliseconds of the last attempt, kept even when the upload failed.
const LAST_TRY: &str = "last_backup_tried_at";
const LAST_SIZE: &str = "last_backup_size";
const LAST_OK: &str = "last_backup_sent";

const SNAPSHOT: &str = "daftar-snapshot.db";
/// The name the database has inside the zip.
const IN_ZIP: &str = "daftar.db";

pub fn handle(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    let _ = args;
    Some(match op {
        "backup.now" => (|| {
            let token = saved_token(ctx)?;
            let transport = Https::new(&token);
            let info = now_with(ctx, &transport)?;
            json_of(info)
        })(),
        "backup.status" => (|| {
            let info = status_with(ctx)?;
            json_of(info)
        })(),
        _ => return None,
    })
}

/// True when the ledger has no backup from today: never backed up, or the last
/// one that reached Telegram is on an earlier Jalali day.
///
/// Called once when the app starts, so it must be cheap and it must never fail:
/// if the database cannot even be read, the answer is "yes, back up" — the safe
/// side of the question.
pub fn due(ctx: &Ctx<'_>) -> bool {
    let last: i64 = ctx
        .conn()
        .ok()
        .map(|conn| db::get_setting(&conn, LAST_SENT, ""))
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    if last <= 0 {
        return true;
    }
    match jalali_day(last) {
        Some(day) => day < db::today(),
        None => true,
    }
}

/// Takes the backup and tries to send it. The zip stays on the phone whatever
/// the network does; `sent` says whether it also left.
pub fn now_with(ctx: &Ctx<'_>, t: &dyn Transport) -> Res<BackupInfo> {
    let (token, chat, shop) = wire(ctx)?;
    let at = db::now_ms();

    // 1. A consistent copy, made by SQLite, then 2. the zip on disk.
    let snapshot = snapshot(ctx)?;
    let archive = zip_snapshot(ctx, &snapshot, at)?;
    let size = std::fs::metadata(&archive).map(|m| m.len() as i64).unwrap_or(0);

    // 3. And only now, the network.
    let (sent, message) = if token.is_empty() || chat.is_empty() {
        (
            false,
            "به تلگرام وصل نیست؛ پشتیبان روی گوشی ماند".to_string(),
        )
    } else {
        let file_name = archive
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "daftar-backup.zip".to_string());
        let caption = if shop.trim().is_empty() {
            "نسخه پشتیبان".to_string()
        } else {
            format!("نسخه پشتیبان {}", shop.trim())
        };
        match std::fs::read(&archive) {
            Ok(bytes) => {
                let fields = [("chat_id", chat.as_str()), ("caption", caption.as_str())];
                match client::send_file(t, "sendDocument", &fields, "document", &file_name, &bytes)
                {
                    Ok(()) => (true, "نسخه پشتیبان به تلگرام رفت".to_string()),
                    // He is told what went wrong *and* that his ledger is safe:
                    // the sentence always ends with the copy on the phone.
                    Err(e) => (
                        false,
                        format!(
                            "{}؛ نسخه پشتیبان روی گوشی ذخیره شد",
                            client::scrub(&e, &token)
                        ),
                    ),
                }
            }
            Err(_) => (
                false,
                "پشتیبان ماند، ولی فرستادنش نشد".to_string(),
            ),
        }
    };

    let info = BackupInfo {
        at,
        size,
        sent,
        message,
    };
    record(ctx, &info)?;
    Ok(info)
}

/// What the last backup was, without making another one.
fn status_with(ctx: &Ctx<'_>) -> Res<BackupInfo> {
    let conn = ctx.conn()?;
    let at: i64 = db::get_setting(&conn, LAST_TRY, "").parse().unwrap_or(0);
    let size: i64 = db::get_setting(&conn, LAST_SIZE, "").parse().unwrap_or(0);
    let sent = db::get_setting(&conn, LAST_OK, "") == "1";
    let message = if at <= 0 {
        "هنوز پشتیبان نگرفته‌ای"
    } else if sent {
        "آخرین نسخه پشتیبان به تلگرام رفت"
    } else {
        "آخرین پشتیبان فقط روی گوشی است"
    };
    Ok(BackupInfo {
        at,
        size,
        sent,
        message: message.to_string(),
    })
}

// ---------------------------------------------------------------------- the files

/// A consistent copy of the live database, made by the engine that owns it.
///
/// `VACUUM INTO` writes a single self-contained file with no WAL beside it and
/// with every page already flushed: the copy is exactly one moment in the
/// ledger's life, which is what a backup has to be.
fn snapshot(ctx: &Ctx<'_>) -> Res<PathBuf> {
    std::fs::create_dir_all(&ctx.state.dirs.files)
        .map_err(|_| "نسخه پشتیبان ذخیره نشد".to_string())?;
    let path = ctx.state.dirs.files.join(SNAPSHOT);
    // `VACUUM INTO` refuses to write over an existing file, and a leftover from
    // a backup that died halfway is stale by definition.
    let _ = std::fs::remove_file(&path);
    {
        let conn = ctx.conn()?;
        conn.execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()])
            .map_err(|_| "ساخت نسخه پشتیبان نشد".to_string())?;
    }
    if !path.exists() {
        return Err("ساخت نسخه پشتیبان نشد".to_string());
    }
    Ok(path)
}

/// Zips the snapshot into the files folder under a dated name, then removes the
/// loose snapshot: the zip is the backup, the copy was only a step.
fn zip_snapshot(ctx: &Ctx<'_>, snapshot: &Path, at: i64) -> Res<PathBuf> {
    let path = ctx.state.dirs.files.join(file_name(at));
    let mut bytes = Vec::new();
    std::fs::File::open(snapshot)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|_| "نسخه پشتیبان ذخیره نشد".to_string())?;

    let file = std::fs::File::create(&path).map_err(|_| "نسخه پشتیبان ذخیره نشد".to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(IN_ZIP, options)
        .map_err(|_| "نسخه پشتیبان ذخیره نشد".to_string())?;
    zip.write_all(&bytes)
        .map_err(|_| "نسخه پشتیبان ذخیره نشد".to_string())?;
    let file = zip
        .finish()
        .map_err(|_| "نسخه پشتیبان ذخیره نشد".to_string())?;
    drop(file);

    let _ = std::fs::remove_file(snapshot);
    Ok(path)
}

/// `daftar-backup-1405-08-08.zip` — the Jalali day in the name, in ASCII so the
/// upload header stays plain and two phones' backups never collide in his chat.
fn file_name(at: i64) -> String {
    match jalali_day(at) {
        Some(day) => {
            let date = crate::jalali::d2j(day);
            format!(
                "daftar-backup-{:04}-{:02}-{:02}.zip",
                date.year, date.month, date.day
            )
        }
        None => "daftar-backup.zip".to_string(),
    }
}

/// Epoch milliseconds as the Jalali day number the rest of the app uses.
fn jalali_day(at: i64) -> Option<i64> {
    let utc = chrono::DateTime::from_timestamp_millis(at)?;
    let local = utc.with_timezone(&chrono::Local);
    Some(crate::jalali::g2d(
        chrono::Datelike::year(&local) as i64,
        chrono::Datelike::month(&local) as i64,
        chrono::Datelike::day(&local) as i64,
    ))
}

// ------------------------------------------------------------------- the plumbing

/// The token and chat he saved, plus the shop name for the caption. The
/// database lock is released before the network is touched.
fn wire(ctx: &Ctx<'_>) -> Res<(String, String, String)> {
    let conn = ctx.conn()?;
    let settings = crate::domain::settings::load(&conn);
    Ok((
        settings.bot_token.trim().to_string(),
        settings.chat_id.trim().to_string(),
        settings.shop_name,
    ))
}

fn saved_token(ctx: &Ctx<'_>) -> Res<String> {
    let conn = ctx.conn()?;
    Ok(crate::domain::settings::load(&conn).bot_token.trim().to_string())
}

/// Remembers how the attempt went. `last_backup_at` is written only when the
/// zip really left the phone: if it did not, the day still counts as unbacked
/// up and the app tries again when it starts.
fn record(ctx: &Ctx<'_>, info: &BackupInfo) -> Res<()> {
    let conn = ctx.conn()?;
    db::put_setting(&conn, LAST_TRY, &info.at.to_string())?;
    db::put_setting(&conn, LAST_SIZE, &info.size.to_string())?;
    db::put_setting(&conn, LAST_OK, if info.sent { "1" } else { "0" })?;
    if info.sent {
        db::put_setting(&conn, LAST_SENT, &info.at.to_string())?;
    }
    Ok(())
}

fn json_of<T: serde::Serialize>(value: T) -> Res<Value> {
    serde_json::to_value(value).map_err(|_| "مشکلی پیش آمد".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::TEST_TOKEN;
    use serde_json::json;
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A transport that records the upload and answers whatever the test says.
    struct Stub {
        answer: Res<Value>,
        uploads: RefCell<Vec<Upload>>,
    }

    struct Upload {
        method: String,
        fields: Vec<(String, String)>,
        file_field: String,
        file_name: String,
        bytes: Vec<u8>,
    }

    impl Stub {
        fn online() -> Self {
            Self {
                answer: Ok(json!({ "ok": true, "result": { "document": { "file_id": "x" } } })),
                uploads: RefCell::new(Vec::new()),
            }
        }

        /// The phone has no network — and, to make the point, the failure text
        /// echoes the token, so the scrubbing is proven too.
        fn offline() -> Self {
            Self {
                answer: Err(format!("اینترنت وصل نیست {TEST_TOKEN}")),
                uploads: RefCell::new(Vec::new()),
            }
        }

        fn last_upload(&self) -> (String, Vec<(String, String)>, String, String, Vec<u8>) {
            let uploads = self.uploads.borrow();
            let upload = uploads.last().expect("an upload was attempted");
            (
                upload.method.clone(),
                upload.fields.clone(),
                upload.file_field.clone(),
                upload.file_name.clone(),
                upload.bytes.clone(),
            )
        }
    }

    impl Transport for Stub {
        fn post_json(&self, _method: &str, _body: &Value) -> Res<Value> {
            Ok(json!({ "ok": true, "result": [] }))
        }

        fn post_file(
            &self,
            method: &str,
            fields: &[(&str, &str)],
            file_field: &str,
            file_name: &str,
            bytes: &[u8],
        ) -> Res<Value> {
            self.uploads.borrow_mut().push(Upload {
                method: method.to_string(),
                fields: fields
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                file_field: file_field.to_string(),
                file_name: file_name.to_string(),
                bytes: bytes.to_vec(),
            });
            self.answer.clone()
        }
    }

    /// A real, file-backed database with a real files folder beside it — the
    /// only way to prove that the snapshot really is a database file.
    struct Bench {
        dir: PathBuf,
        state: Option<crate::AppState>,
    }

    impl Bench {
        fn new(tag: &str) -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("daftar-{tag}-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("temp dir");
            // The app makes this folder when it starts; a test bench should
            // look the same, including for the ops that write nothing.
            std::fs::create_dir_all(dir.join("files")).expect("files dir");
            let conn = db::open(&dir.join("daftar.db")).expect("a real database");
            let state = crate::AppState {
                db: std::sync::Mutex::new(conn),
                dirs: crate::Dirs {
                    data: dir.clone(),
                    files: dir.join("files"),
                },
            };
            Self {
                dir,
                state: Some(state),
            }
        }

        fn state(&self) -> &crate::AppState {
            self.state.as_ref().expect("state")
        }

        fn ctx(&self) -> Ctx<'_> {
            Ctx { state: self.state() }
        }

        fn set(&self, key: &str, value: &str) {
            let conn = self.state().conn().unwrap();
            db::put_setting(&conn, key, value).unwrap();
        }

        fn get(&self, key: &str) -> String {
            let conn = self.state().conn().unwrap();
            db::get_setting(&conn, key, "")
        }

        fn wired(&self) {
            self.set("bot_token", TEST_TOKEN);
            self.set("chat_id", "777");
            self.set("shop_name", "دکان رضا");
        }

        /// The one zip left in the files folder.
        fn zips(&self) -> Vec<PathBuf> {
            std::fs::read_dir(&self.state().dirs.files)
                .expect("files folder")
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| path.extension().map(|e| e == "zip").unwrap_or(false))
                .collect()
        }
    }

    impl Drop for Bench {
        fn drop(&mut self) {
            // Close SQLite first: Windows will not delete an open file. The
            // handle can take a moment to be released, so try a few times —
            // a leftover temp folder is untidy, never a test failure.
            self.state = None;
            for _ in 0..10 {
                if std::fs::remove_dir_all(&self.dir).is_ok() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
    }

    #[test]
    fn backup_now_zips_the_database_and_sends_it() {
        let bench = Bench::new("backup-one");
        bench.wired();
        // Something in the ledger, so the copy is of a real page.
        {
            let conn = bench.state().conn().unwrap();
            conn.execute(
                "INSERT INTO party(id, name, kind, opening, created_at, updated_at)
                 VALUES(1, 'لیلا امینی', 'customer', 0, 0, 0)",
                [],
            )
            .unwrap();
        }
        let stub = Stub::online();
        let info = now_with(&bench.ctx(), &stub).unwrap();

        assert!(info.sent, "telegram answered, so it was sent");
        assert_eq!(info.message, "نسخه پشتیبان به تلگرام رفت");
        assert!(info.size > 0, "the zip has bytes in it");

        // The zip is on disk and holds a database file.
        let zips = bench.zips();
        assert_eq!(zips.len(), 1, "one zip, not none and not three");
        assert_eq!(info.size, std::fs::metadata(&zips[0]).unwrap().len() as i64);
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&zips[0]).unwrap()).unwrap();
        {
            let mut entry = archive.by_name(IN_ZIP).expect("daftar.db is inside");
            let mut head = [0u8; 16];
            entry.read_exact(&mut head).unwrap();
            assert_eq!(&head, b"SQLite format 3\0", "a real SQLite file");
        }

        // It went to his own chat, as a document, with the shop in the caption.
        let (method, fields, file_field, file_name, bytes) = stub.last_upload();
        assert_eq!(method, "sendDocument");
        assert_eq!(file_field, "document");
        assert!(file_name.ends_with(".zip"), "name: {file_name}");
        assert!(fields.contains(&("chat_id".to_string(), "777".to_string())));
        let caption = fields
            .iter()
            .find(|(key, _)| key == "caption")
            .map(|(_, value)| value.clone())
            .expect("a caption");
        assert!(caption.contains("نسخه پشتیبان"), "caption: {caption}");
        assert!(caption.contains("دکان رضا"), "caption: {caption}");
        assert_eq!(bytes, std::fs::read(&zips[0]).unwrap(), "the zip left whole");

        // And the settings know when it happened.
        let at: i64 = bench.get(LAST_SENT).parse().unwrap();
        assert!(at > 0, "last_backup_at was recorded");
        assert_eq!(at, info.at);
    }

    #[test]
    fn backup_now_keeps_the_zip_when_there_is_no_network() {
        let bench = Bench::new("backup-offline");
        bench.wired();
        let stub = Stub::offline();
        let info = now_with(&bench.ctx(), &stub).unwrap();

        assert!(!info.sent, "nothing reached telegram");
        assert!(
            info.message.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c)),
            "a Persian sentence, not an english error: {}",
            info.message
        );
        assert!(
            !info.message.contains(TEST_TOKEN),
            "the token is not in a message: {}",
            info.message
        );

        // The data is never lost because the network was down: the zip is
        // there, whole, and a second attempt will send that same copy.
        let zips = bench.zips();
        assert_eq!(zips.len(), 1);
        assert!(std::fs::metadata(&zips[0]).unwrap().len() > 0);
        let archive = zip::ZipArchive::new(std::fs::File::open(&zips[0]).unwrap());
        assert!(archive.is_ok(), "the zip on disk is readable");

        // The day still counts as unbacked up, so the app tries again.
        assert_eq!(bench.get(LAST_SENT), "", "no success was recorded");
        assert_eq!(bench.get(LAST_OK), "0");
        assert!(due(&bench.ctx()), "still due after a failed send");
    }

    #[test]
    fn due_is_true_before_a_backup_and_false_after_one() {
        let bench = Bench::new("due");
        bench.wired();
        assert!(due(&bench.ctx()), "a fresh ledger wants a backup");

        let info = now_with(&bench.ctx(), &Stub::online()).unwrap();
        assert!(info.sent);
        assert!(!due(&bench.ctx()), "the backup that just went is today's");

        // A backup from an earlier Jalali day is due again: 36 hours back is
        // always a whole day behind, whatever the hour is now.
        let earlier = db::now_ms() - 36 * 60 * 60 * 1000;
        assert_ne!(
            jalali_day(earlier),
            jalali_day(db::now_ms()),
            "36 hours back is another day"
        );
        bench.set(LAST_SENT, &earlier.to_string());
        assert!(due(&bench.ctx()), "yesterday's backup does not count");
    }

    #[test]
    fn status_describes_the_last_backup_without_making_one() {
        let bench = Bench::new("status");
        bench.wired();

        // Never backed up: it says so, and it says it in Persian.
        let info = status_with(&bench.ctx()).unwrap();
        assert_eq!(info.at, 0);
        assert!(!info.sent);
        assert_eq!(info.message, "هنوز پشتیبان نگرفته‌ای");
        assert!(bench.zips().is_empty(), "status makes no files");

        // After a real backup it reports that one.
        let sent = now_with(&bench.ctx(), &Stub::online()).unwrap();
        let info = status_with(&bench.ctx()).unwrap();
        assert_eq!(info.at, sent.at);
        assert_eq!(info.size, sent.size);
        assert!(info.sent);
        assert_eq!(info.message, "آخرین نسخه پشتیبان به تلگرام رفت");
    }

    #[test]
    fn a_backup_without_a_bot_still_leaves_a_zip() {
        let bench = Bench::new("no-bot");
        // Nothing wired: no token, no chat.
        let stub = Stub::online();
        let info = now_with(&bench.ctx(), &stub).unwrap();

        assert!(!info.sent, "there is nowhere to send it yet");
        assert!(info.message.contains("تلگرام"), "message: {}", info.message);
        assert!(stub.uploads.borrow().is_empty(), "nothing was uploaded");
        assert_eq!(bench.zips().len(), 1, "the copy is still made");
    }
}
