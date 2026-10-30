//! The harness the end-to-end tests share.
//!
//! Every test here has to see the ledger exactly as the interface sees it, so
//! this module builds a real database from the schema the app ships and then
//! talks to it through [`daftar_lib::route`] — the single door `call` in
//! `lib.rs` uses. No test touches a domain module, a struct or a table
//! directly: if a number is wrong, it is wrong on the screen too.
//!
//! The schema is compiled in with `include_str!` because `db` is a private
//! module of the library: `db::memory()` is `#[cfg(test)]` and `db::open` is
//! private, so neither is reachable from an integration test. The file is the
//! one the app runs, so there is still one source of truth.

#![allow(dead_code)]

use daftar_lib::{route, AppState, Ctx, Dirs};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

/// `src-tauri/src/schema.sql`, the schema the app itself executes at start-up.
const SCHEMA: &str = include_str!("../../src/schema.sql");

// ---------------------------------------------------------------------------
// the ledger
// ---------------------------------------------------------------------------

/// A ledger plus the folders that live beside it, on disk or in memory.
///
/// The state is held in an `Option` so [`Drop`] can close SQLite before the
/// temporary folder is removed: Windows will not delete an open file.
pub struct Ledger {
    state: Option<AppState>,
    dir: Option<PathBuf>,
}

impl Ledger {
    /// An empty ledger in memory — fast, isolated, and enough for every flow
    /// that does not need a file (only the restart flow needs one).
    pub fn memory() -> Self {
        let conn = Connection::open_in_memory().expect("in-memory database");
        conn.pragma_update(None, "foreign_keys", true).ok();
        conn.execute_batch(SCHEMA).expect("the shipped schema applies");
        Ledger {
            state: Some(AppState {
                db: Mutex::new(conn),
                dirs: Dirs {
                    data: PathBuf::new(),
                    files: PathBuf::new(),
                },
            }),
            dir: None,
        }
    }

    /// A ledger with a real database file and a real `files` folder, set up the
    /// way the app sets itself up at start-up. The folder is removed on drop.
    pub fn on_disk(tag: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "daftar-e2e-{tag}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("files")).expect("the temp folders");
        let conn = file_connection(&dir.join("daftar.db"));
        Ledger {
            state: Some(AppState {
                db: Mutex::new(conn),
                dirs: Dirs {
                    data: dir.clone(),
                    files: dir.join("files"),
                },
            }),
            dir: Some(dir),
        }
    }

    /// The same ledger opened again from a database file: a restart, with the
    /// folders of the ledger it came from.
    pub fn reopen(&self, path: &Path) -> Ledger {
        let dirs = {
            let dirs = &self.state().dirs;
            Dirs {
                data: dirs.data.clone(),
                files: dirs.files.clone(),
            }
        };
        Ledger {
            state: Some(AppState {
                db: Mutex::new(file_connection(path)),
                dirs,
            }),
            // The folder belongs to the ledger this one was opened from.
            dir: None,
        }
    }

    /// The ledger's own connection, for the one test that hands a `VACUUM INTO`
    /// snapshot to a fresh reader. Everything else goes through [`Ledger::call`].
    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.state().db.lock().expect("the database lock")
    }

    pub fn state(&self) -> &AppState {
        self.state.as_ref().expect("the ledger is open")
    }

    pub fn files_dir(&self) -> PathBuf {
        self.state().dirs.files.clone()
    }

    /// One operation, exactly as the interface asks for it.
    ///
    /// A failed operation is a failed test: the shopkeeper may never see
    /// anything but a Persian sentence, and neither may this harness.
    pub fn call(&self, op: &str, args: Value) -> Value {
        match self.attempt(op, args.clone()) {
            Ok(value) => value,
            Err(e) => panic!("«{op}» جواب نداد: {e}\n  args: {args}"),
        }
    }

    /// One operation that is allowed to fail, for the tests that prove an error
    /// is refused rather than swallowed.
    pub fn attempt(&self, op: &str, args: Value) -> Result<Value, String> {
        let ctx = Ctx { state: self.state() };
        route(&ctx, op, &args)
    }
}

impl Drop for Ledger {
    fn drop(&mut self) {
        // Close SQLite first, then take the folder away. A leftover folder in
        // the temp directory is untidy, never a failed test.
        self.state = None;
        if let Some(dir) = self.dir.take() {
            for _ in 0..20 {
                if std::fs::remove_dir_all(&dir).is_ok() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
    }
}

fn file_connection(path: &Path) -> Connection {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("the folder of the database file");
    }
    let conn = Connection::open(path).expect("a database file");
    // The same durability settings the app opens with.
    conn.pragma_update(None, "journal_mode", "WAL").ok();
    conn.pragma_update(None, "synchronous", "NORMAL").ok();
    conn.pragma_update(None, "foreign_keys", true).ok();
    conn.execute_batch(SCHEMA).expect("the shipped schema applies");
    conn
}

// ---------------------------------------------------------------------------
// reading the JSON the interface returns
// ---------------------------------------------------------------------------

/// A number out of a JSON object, with a message that says which field was
/// missing instead of panicking somewhere inside serde.
pub fn n(value: &Value, key: &str) -> i64 {
    value
        .get(key)
        .and_then(Value::as_i64)
        .unwrap_or_else(|| panic!("«{key}» در این پاسخ نیست یا عدد نیست: {value}"))
}

pub fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("«{key}» در این پاسخ نیست یا متن نیست: {value}"))
}

/// The day the ledger thinks it is, from the interface's own clock. Every test
/// date is relative to this so nothing depends on the day the suite runs.
pub fn today(ledger: &Ledger) -> i64 {
    n(&ledger.call("home.today", json!({ "day": null })), "day")
}

/// The one number on a party's page: positive they owe me, negative I owe them.
pub fn balance(ledger: &Ledger, party: i64) -> i64 {
    n(&ledger.call("parties.book", json!({ "id": party })), "balance")
}

/// The two numbers on the home banner: `(owes_me, i_owe)`.
pub fn totals(ledger: &Ledger) -> (i64, i64) {
    let t = ledger.call("parties.totals", json!({}));
    (n(&t, "owesMe"), n(&t, "iOwe"))
}

/// Every document of one party, newest first, each with its own `paid`,
/// `remaining` and `status` worked out by the app.
pub fn docs_of(ledger: &Ledger, party: i64) -> Vec<Value> {
    let rows = ledger.call(
        "orders.list",
        json!({ "partyId": party, "limit": 1000 }),
    );
    rows.as_array()
        .expect("a list of documents")
        .iter()
        .map(|row| row["doc"].clone())
        .collect()
}

/// Every payment of one party, newest first, with `allocated` and
/// `unallocated` beside each one.
pub fn payments_of(ledger: &Ledger, party: i64) -> Vec<Value> {
    let rows = ledger.call("payments.list", json!({ "partyId": party, "limit": 1000 }));
    rows.as_array().expect("a list of payments").to_vec()
}

// ---------------------------------------------------------------------------
// sending what a screen sends
// ---------------------------------------------------------------------------

/// One line of the order screen, in the shape `api.ts` hands to `orders.save`:
/// camelCase, quantities in thousandths, money in toman.
pub fn line(name: &str, qty_milli: i64, price: i64) -> Value {
    json!({
        "name": name,
        "qtyMilli": qty_milli,
        "unit": "kg",
        "price": price,
        "voided": false
    })
}

/// The whole `DocInput` a sale screen sends. The defaults are the quiet ones:
/// no discount, no rounding, no cash at the counter, not final.
pub fn doc_input(party: i64, day: i64, lines: Vec<Value>) -> Value {
    json!({
        "kind": "sale",
        "partyId": party,
        "day": day,
        "discount": 0,
        "roundStep": 0,
        "final": false,
        "lines": lines,
        "receivedNow": 0,
        "receivedMethod": "cash",
        "updatePrices": false
    })
}

/// The same, as a purchase from the source.
pub fn purchase_input(party: i64, day: i64, lines: Vec<Value>) -> Value {
    let mut input = doc_input(party, day, lines);
    input["kind"] = json!("purchase");
    input
}

/// `orders.save` and the document it returns; an id of zero is never a document.
pub fn save_doc(ledger: &Ledger, input: Value) -> Value {
    let doc = ledger.call("orders.save", json!({ "input": input }));
    assert!(
        n(&doc, "id") > 0,
        "orders.save did not make a document: {doc}"
    );
    doc
}

/// `orders.get`: the document, its party and its lines, as the invoice screen
/// reads it back.
pub fn get_doc(ledger: &Ledger, id: i64) -> Value {
    ledger.call("orders.get", json!({ "id": id }))
}

/// A new party, by the only thing a party must have: a name.
pub fn party(ledger: &Ledger, name: &str) -> i64 {
    party_of_kind(ledger, name, None)
}

pub fn party_of_kind(ledger: &Ledger, name: &str, kind: Option<&str>) -> i64 {
    let mut args = json!({ "party": { "name": name } });
    if let Some(kind) = kind {
        args["party"]["kind"] = json!(kind);
    }
    let saved = ledger.call("parties.save", args);
    n(&saved, "id")
}

/// `payments.save` with no allocation list: the app spreads the money over the
/// oldest unpaid documents of the matching kind and leaves the rest as credit.
pub fn pay(ledger: &Ledger, party: i64, direction: &str, amount: i64, day: i64) -> Value {
    pay_with(ledger, party, direction, amount, day, Vec::new())
}

/// `payments.save` with the allocations the payment screen typed by hand.
pub fn pay_with(
    ledger: &Ledger,
    party: i64,
    direction: &str,
    amount: i64,
    day: i64,
    allocations: Vec<(i64, i64)>,
) -> Value {
    let allocations: Vec<Value> = allocations
        .into_iter()
        .map(|(doc_id, amount)| json!({ "docId": doc_id, "amount": amount }))
        .collect();
    let payment = ledger.call(
        "payments.save",
        json!({ "input": {
            "partyId": party,
            "direction": direction,
            "amount": amount,
            "day": day,
            "method": "cash",
            "allocations": allocations
        }}),
    );
    assert!(
        n(&payment, "id") > 0,
        "payments.save did not record a payment: {payment}"
    );
    payment
}

/// `cheques.save`, in the shape the cheque screen sends.
pub fn save_cheque(ledger: &Ledger, party: i64, amount: i64, due_day: i64) -> Value {
    ledger.call(
        "cheques.save",
        json!({ "cheque": {
            "partyId": party,
            "direction": "in",
            "number": "۱۲۳۴۵۶",
            "bank": "صادرات",
            "amount": amount,
            "dueDay": due_day
        }}),
    )
}

/// `cheques.status` — the interface sends no day, and the app uses today.
pub fn cheque_status(ledger: &Ledger, id: i64, status: &str) -> Value {
    ledger.call("cheques.status", json!({ "id": id, "status": status }))
}

/// The amount the shopkeeper would read off an invoice, formatted the way the
/// app formats money — used to prove the search box accepts it.
pub fn fa_amount(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push('\u{066C}'); // ٬ ARABIC THOUSANDS SEPARATOR
        }
        out.push(ch);
    }
    let fa: String = out
        .chars()
        .map(|c| match c {
            '0'..='9' => char::from_u32('۰' as u32 + (c as u32 - '0' as u32)).unwrap_or(c),
            other => other,
        })
        .collect();
    if n < 0 {
        format!("−{fa}")
    } else {
        fa
    }
}
