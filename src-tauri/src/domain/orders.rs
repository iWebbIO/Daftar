//! Invoices: the order screen, and everything saving one writes.
//!
//! A sale and a purchase are the same document with a different `kind`, so this
//! is the only place a document is saved, numbered, rounded, priced or voided.
//! Every write here happens inside one transaction: a half-saved invoice is the
//! one thing a paper notebook can never do.

use crate::db::{self, Res};
use crate::domain::{balance, parties, payments};
use crate::dto::{AllocationInput, Doc, DocFull, DocInput, DocLine, DocRow, LineInput, PaymentInput};
use crate::jalali;
use crate::money;
use crate::Ctx;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// The document columns every read shares, with `paid` worked out from the
/// allocations so `remaining` is never stale.
pub const DOC_COLUMNS: &str = r#"
d.id, d.kind, d.jyear, d.number, d.party_id, d.day, d.status, d.discount, d.rounding,
d.note, d.total, d.final, d.void_reason, d.created_at, d.updated_at,
COALESCE((SELECT SUM(a.amount) FROM allocation a JOIN payment pm ON pm.id = a.payment_id
          WHERE a.doc_id = d.id AND pm.voided = 0), 0)
"#;

pub fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Doc> {
    let total: i64 = r.get(10)?;
    let paid: i64 = r.get(15)?;
    Ok(Doc {
        id: r.get(0)?,
        kind: r.get(1)?,
        jyear: r.get(2)?,
        number: r.get(3)?,
        party_id: r.get(4)?,
        day: r.get(5)?,
        status: r.get(6)?,
        discount: r.get(7)?,
        rounding: r.get(8)?,
        note: r.get(9)?,
        total,
        paid,
        remaining: total - paid,
        is_final: r.get::<_, i64>(11)? != 0,
        void_reason: r.get(12)?,
        created_at: r.get(13)?,
        updated_at: r.get(14)?,
    })
}

/// Documents on one day, oldest first, with the party's name.
pub fn rows_on_day(conn: &Connection, day: i64) -> Res<Vec<DocRow>> {
    let sql = format!(
        "SELECT {DOC_COLUMNS}, p.name FROM doc d JOIN party p ON p.id = d.party_id
         WHERE d.day = ?1 AND d.status <> 'void' ORDER BY d.id"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| crate::db::oops("دفتر خوانده نشد", e))?;
    let rows = stmt
        .query_map([day], |r| {
            Ok(DocRow {
                doc: from_row(r)?,
                party_name: r.get(16)?,
            })
        })
        .map_err(|e| crate::db::oops("دفتر خوانده نشد", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| crate::db::oops("دفتر خوانده نشد", e))?;
    Ok(rows)
}

/// The one way a write happens: a transaction that either lands whole or not at
/// all. Every module that saves money uses this, so a save can never be half a
/// save on a phone that dies mid-keystroke.
pub fn in_tx<T>(conn: &Connection, f: impl FnOnce(&Connection) -> Res<T>) -> Res<T> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| crate::db::oops("شروع ذخیره نشد", e))?;
    match f(&tx) {
        Ok(value) => {
            tx.commit().map_err(|e| crate::db::oops("ذخیره کامل نشد", e))?;
            Ok(value)
        }
        Err(e) => {
            let _ = tx.rollback();
            Err(e)
        }
    }
}

/// `qty_milli × price / 1000`, rounded half away from zero. This is the multiply
/// the notebook does in the shopkeeper's head, done identically every time.
pub fn line_total(qty_milli: i64, price: i64) -> i64 {
    let raw = qty_milli as i128 * price as i128;
    let rounded = if raw >= 0 {
        (raw + 500) / 1000
    } else {
        -((-raw + 500) / 1000)
    };
    rounded as i64
}

fn blank(v: &Option<String>) -> Option<String> {
    v.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// reading
// ---------------------------------------------------------------------------

/// One document, with `paid` and `remaining` worked out live.
pub fn load(conn: &Connection, id: i64) -> Res<Doc> {
    let sql = format!("SELECT {DOC_COLUMNS} FROM doc d WHERE d.id = ?1");
    conn.query_row(&sql, [id], from_row)
        .optional()
        .map_err(|e| crate::db::oops("خواندن فاکتور نشد", e))?
        .ok_or_else(|| "فاکتور پیدا نشد".to_string())
}

/// The lines of a document, in the order they were written. Crossed-out lines
/// are included: the shopkeeper must still be able to read what he crossed out.
pub fn lines_of(conn: &Connection, doc_id: i64) -> Res<Vec<DocLine>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, doc_id, pos, product_id, name, qty_milli, unit, price, total,
                    override_total, note, voided
             FROM doc_line WHERE doc_id = ?1 ORDER BY pos, id",
        )
        .map_err(|e| crate::db::oops("خواندن سطرها نشد", e))?;
    let rows = stmt
        .query_map([doc_id], |r| {
            Ok(DocLine {
                id: r.get(0)?,
                doc_id: r.get(1)?,
                pos: r.get(2)?,
                product_id: r.get(3)?,
                name: r.get(4)?,
                qty_milli: r.get(5)?,
                unit: r.get(6)?,
                price: r.get(7)?,
                total: r.get(8)?,
                override_total: r.get(9)?,
                note: r.get(10)?,
                voided: r.get::<_, i64>(11)? != 0,
            })
        })
        .map_err(|e| crate::db::oops("خواندن سطرها نشد", e))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| crate::db::oops("خواندن سطرها نشد", e))?);
    }
    Ok(out)
}

pub fn full(conn: &Connection, id: i64) -> Res<DocFull> {
    let doc = load(conn, id)?;
    let party = parties::party_of(conn, doc.party_id)?;
    let lines = lines_of(conn, id)?;
    Ok(DocFull { doc, party, lines })
}

/// The next number in this kind and Jalali year. A number is never handed out
/// twice and an existing document is never renumbered, so the notebook's teeth
/// stay in the same order as the app's.
fn next_number(conn: &Connection, kind: &str, jyear: i64) -> Res<i64> {
    conn.query_row(
        "SELECT COALESCE(MAX(number), 0) + 1 FROM doc WHERE kind = ?1 AND jyear = ?2",
        params![kind, jyear],
        |r| r.get(0),
    )
    .map_err(|e| crate::db::oops("شماره فاکتور نشد", e))
}

/// The last price this party was charged for a product, if it was ever typed.
fn last_price(conn: &Connection, product_id: i64, party_id: i64, kind: &str) -> Res<Option<i64>> {
    conn.query_row(
        "SELECT price FROM price_history
         WHERE product_id = ?1 AND party_id = ?2 AND kind = ?3
         ORDER BY day DESC, at DESC, id DESC LIMIT 1",
        params![product_id, party_id, kind],
        |r| r.get(0),
    )
    .optional()
    .map_err(|e| crate::db::oops("خواندن قیمت قبلی نشد", e))
}

fn catalog_price(conn: &Connection, product_id: i64, kind: &str) -> Res<Option<i64>> {
    let column = if kind == "purchase" { "buy_price" } else { "sale_price" };
    conn.query_row(
        &format!("SELECT {column} FROM product WHERE id = ?1"),
        [product_id],
        |r| r.get(0),
    )
    .optional()
    .map_err(|e| crate::db::oops("خواندن قیمت کالا نشد", e))
}

/// A line checked against the catalog: a product that has since been removed
/// must not break the save, so its id is dropped rather than forced through.
fn resolve_line(conn: &Connection, line: &LineInput) -> Res<(Option<i64>, String, String)> {
    let mut product_id = line.product_id.filter(|id| *id > 0);
    let mut catalog: Option<(String, String)> = None;
    if let Some(pid) = product_id {
        catalog = conn
            .query_row("SELECT name, unit FROM product WHERE id = ?1", [pid], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .optional()
            .map_err(|e| crate::db::oops("خواندن کالا نشد", e))?;
        if catalog.is_none() {
            product_id = None;
        }
    }
    let name = match line.name.trim() {
        "" => catalog.as_ref().map(|c| c.0.clone()).unwrap_or_default(),
        other => other.to_string(),
    };
    if name.trim().is_empty() {
        return Err("نام کالا را بنویس".to_string());
    }
    let unit = match line.unit.trim() {
        "" => catalog
            .as_ref()
            .map(|c| c.1.clone())
            .unwrap_or_else(|| "kg".to_string()),
        other => other.to_string(),
    };
    Ok((product_id, name, unit))
}

// ---------------------------------------------------------------------------
// writing
// ---------------------------------------------------------------------------

/// Saves one invoice or purchase. Returns the document as the interface will
/// print it, recomputed from what actually landed in the database.
pub fn save(conn: &Connection, input: &DocInput) -> Res<Doc> {
    let kind = match input.kind.as_str() {
        "sale" => "sale",
        "purchase" => "purchase",
        _ => return Err("نوع فاکتور نامشخص است".to_string()),
    };
    if input.discount < 0 {
        return Err("تخفیف را درست بزن".to_string());
    }
    if input.lines.is_empty() {
        return Err("چیزی به فاکتور اضافه نشده".to_string());
    }
    let day = if input.day > 0 { input.day } else { db::today() };
    let note = blank(&input.note);

    in_tx(conn, |conn| {
        parties::party_of(conn, input.party_id)?;
        let now = db::now_ms();

        let current = match input.id.filter(|id| *id > 0) {
            Some(id) => {
                let doc = load(conn, id)?;
                if doc.status == "void" {
                    return Err("فاکتور باطل‌شده را نمی‌شود عوض کرد".to_string());
                }
                if doc.kind != kind {
                    return Err("نوع فاکتور را نمی‌شود عوض کرد".to_string());
                }
                Some(doc)
            }
            None => None,
        };

        // The money: every line rounded to the toman, the crossed-out ones left
        // out of the sum, the discount once, the rounding once — on the total.
        let mut sum = 0i64;
        let mut resolved: Vec<(Option<i64>, String, String, i64)> = Vec::with_capacity(input.lines.len());
        for line in &input.lines {
            let (product_id, name, unit) = resolve_line(conn, line)?;
            let total = match line.override_total {
                Some(typed) => typed,
                None => line_total(line.qty_milli, line.price),
            };
            if !line.voided {
                sum += total;
            }
            resolved.push((product_id, name, unit, total));
        }
        let before = sum - input.discount;
        let total = money::round_to(before, input.round_step);
        let rounding = total - before;

        let (doc_id, jyear, number) = match &current {
            Some(doc) => {
                conn.execute(
                    "UPDATE doc SET party_id = ?1, day = ?2, discount = ?3, rounding = ?4,
                            note = ?5, total = ?6, final = ?7, updated_at = ?8
                     WHERE id = ?9",
                    params![
                        input.party_id,
                        day,
                        input.discount,
                        rounding,
                        note,
                        total,
                        input.is_final as i64,
                        now,
                        doc.id
                    ],
                )
                .map_err(|e| crate::db::oops("ذخیره فاکتور نشد", e))?;
                (doc.id, doc.jyear, doc.number)
            }
            None => {
                let jyear = jalali::d2j(day).year;
                let number = next_number(conn, kind, jyear)?;
                conn.execute(
                    "INSERT INTO doc(kind, jyear, number, party_id, day, status, discount, rounding,
                                     note, total, final, created_at, updated_at)
                     VALUES(?1, ?2, ?3, ?4, ?5, 'open', ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        kind,
                        jyear,
                        number,
                        input.party_id,
                        day,
                        input.discount,
                        rounding,
                        note,
                        total,
                        input.is_final as i64,
                        now,
                        now
                    ],
                )
                .map_err(|e| crate::db::oops("ذخیره فاکتور نشد", e))?;
                (conn.last_insert_rowid(), jyear, number)
            }
        };

        // The lines. A line the shopkeeper deleted on an edit is crossed out,
        // not erased: the book has to keep showing what was written.
        let keep: HashSet<i64> = input
            .lines
            .iter()
            .filter_map(|l| l.id.filter(|id| *id > 0))
            .collect();
        if current.is_some() {
            let mut stmt = conn
                .prepare("SELECT id FROM doc_line WHERE doc_id = ?1")
                .map_err(|e| crate::db::oops("خواندن سطرها نشد", e))?;
            let ids = stmt
                .query_map([doc_id], |r| r.get::<_, i64>(0))
                .map_err(|e| crate::db::oops("خواندن سطرها نشد", e))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| crate::db::oops("خواندن سطرها نشد", e))?;
            for old in ids {
                if !keep.contains(&old) {
                    conn.execute("UPDATE doc_line SET voided = 1 WHERE id = ?1", [old])
                        .map_err(|e| crate::db::oops("خط‌خوردن سطر نشد", e))?;
                }
            }
        }

        for (pos, (line, (product_id, name, unit, lt))) in
            input.lines.iter().zip(resolved.iter()).enumerate()
        {
            let existing = line
                .id
                .filter(|id| *id > 0)
                .filter(|_| current.is_some());
            let mut changed = 0usize;
            if let Some(lid) = existing {
                changed = conn
                    .execute(
                        "UPDATE doc_line SET pos = ?1, product_id = ?2, name = ?3, qty_milli = ?4,
                                unit = ?5, price = ?6, total = ?7, override_total = ?8, note = ?9,
                                voided = ?10
                         WHERE id = ?11 AND doc_id = ?12",
                        params![
                            pos as i64,
                            product_id,
                            name,
                            line.qty_milli,
                            unit,
                            line.price,
                            lt,
                            line.override_total,
                            blank(&line.note),
                            line.voided as i64,
                            lid,
                            doc_id
                        ],
                    )
                    .map_err(|e| crate::db::oops("ذخیره سطر نشد", e))?;
            }
            if changed == 0 {
                conn.execute(
                    "INSERT INTO doc_line(doc_id, pos, product_id, name, qty_milli, unit, price,
                                         total, override_total, note, voided)
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        doc_id,
                        pos as i64,
                        product_id,
                        name,
                        line.qty_milli,
                        unit,
                        line.price,
                        lt,
                        line.override_total,
                        blank(&line.note),
                        line.voided as i64
                    ],
                )
                .map_err(|e| crate::db::oops("ذخیره سطر نشد", e))?;
            }
        }

        // The memory of prices: a row every time a price is not the last one
        // this party was charged, whether or not the catalog is updated.
        let history_kind = if kind == "sale" { "sale" } else { "buy" };
        let mut latest: HashMap<i64, i64> = HashMap::new();
        for (line, (product_id, _, _, _)) in input.lines.iter().zip(resolved.iter()) {
            let Some(pid) = *product_id else { continue };
            if line.voided {
                continue;
            }
            let previous = match latest.get(&pid) {
                Some(price) => Some(*price),
                None => {
                    let found = last_price(conn, pid, input.party_id, history_kind)?;
                    if let Some(price) = found {
                        latest.insert(pid, price);
                    }
                    found
                }
            };
            if previous != Some(line.price) {
                conn.execute(
                    "INSERT INTO price_history(product_id, party_id, kind, price, day, at)
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                    params![pid, input.party_id, history_kind, line.price, day, now],
                )
                .map_err(|e| crate::db::oops("ثبت قیمت نشد", e))?;
                latest.insert(pid, line.price);
            }
        }

        if input.update_prices {
            let column = if kind == "sale" { "sale_price" } else { "buy_price" };
            for (line, (product_id, _, _, _)) in input.lines.iter().zip(resolved.iter()) {
                let Some(pid) = *product_id else { continue };
                if line.voided {
                    continue;
                }
                conn.execute(
                    &format!("UPDATE product SET {column} = ?1, updated_at = ?2 WHERE id = ?3"),
                    params![line.price, now, pid],
                )
                .map_err(|e| crate::db::oops("ذخیره قیمت کالا نشد", e))?;
            }
        }

        // The status is never typed: it is what the money says it is.
        refresh_status(conn, doc_id)?;

        // Cash handed over at the counter, recorded as a real payment so it
        // shows on the party's page like any other.
        if input.received_now > 0 {
            let paid = balance::paid_on(conn, doc_id)?;
            let apply = input.received_now.min((total - paid).max(0));
            let method = match input.received_method.trim() {
                "" => "cash".to_string(),
                other => other.to_string(),
            };
            payments::record(
                conn,
                &PaymentInput {
                    id: None,
                    party_id: input.party_id,
                    direction: if kind == "sale" { "in" } else { "out" }.to_string(),
                    amount: input.received_now,
                    day,
                    method,
                    reference: None,
                    note: Some(format!(
                        "همراه فاکتور {}",
                        money::fa_digits(&number.to_string())
                    )),
                    allocations: if apply > 0 {
                        vec![AllocationInput {
                            doc_id,
                            amount: apply,
                        }]
                    } else {
                        Vec::new()
                    },
                },
                false,
            )?;
            refresh_status(conn, doc_id)?;
        }

        let action = if current.is_some() { "update" } else { "create" };
        db::audit(
            conn,
            "doc",
            doc_id,
            action,
            &json!({
                "kind": kind,
                "jyear": jyear,
                "number": number,
                "day": day,
                "total": total,
                "discount": input.discount,
                "rounding": rounding,
                "lines": input.lines.len(),
                "received": input.received_now,
            })
            .to_string(),
        )?;

        load(conn, doc_id)
    })
}

/// `settled` when the allocations cover the total, `open` when they do not, and
/// `void` stays `void` forever. Called after anything that moves money.
pub fn refresh_status(conn: &Connection, doc_id: i64) -> Res<()> {
    let paid = balance::paid_on(conn, doc_id)?;
    conn.execute(
        "UPDATE doc SET status = CASE WHEN status = 'void' THEN 'void'
                                      WHEN ?1 >= total THEN 'settled'
                                      ELSE 'open' END
         WHERE id = ?2",
        params![paid, doc_id],
    )
    .map_err(|e| crate::db::oops("ثبت وضعیت فاکتور نشد", e))?;
    Ok(())
}

/// Crosses out a document. The lines stay readable, the balance stops counting
/// it, and any money that had been applied to it goes back to the party as
/// credit — cash paid against an invoice that no longer exists must not vanish.
pub fn void(conn: &Connection, id: i64, reason: &str) -> Res<Doc> {
    let reason = reason.trim().to_string();
    if reason.is_empty() {
        return Err("دلیلش را بنویس".to_string());
    }
    in_tx(conn, |conn| {
        let doc = load(conn, id)?;
        if doc.status == "void" {
            return Err("این فاکتور قبلاً باطل شده".to_string());
        }
        let mut released: Vec<(i64, i64)> = Vec::new();
        {
            let mut stmt = conn
                .prepare(
                    "SELECT a.payment_id, a.amount FROM allocation a
                     JOIN payment pm ON pm.id = a.payment_id
                     WHERE a.doc_id = ?1 AND pm.voided = 0",
                )
                .map_err(|e| crate::db::oops("خواندن پرداخت‌ها نشد", e))?;
            let rows = stmt
                .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
                .map_err(|e| crate::db::oops("خواندن پرداخت‌ها نشد", e))?;
            for row in rows {
                released.push(row.map_err(|e| crate::db::oops("خواندن پرداخت‌ها نشد", e))?);
            }
        }
        conn.execute("DELETE FROM allocation WHERE doc_id = ?1", [id])
            .map_err(|e| crate::db::oops("آزاد کردن پرداخت نشد", e))?;
        conn.execute(
            "UPDATE doc SET status = 'void', void_reason = ?1, updated_at = ?2 WHERE id = ?3",
            params![reason, db::now_ms(), id],
        )
        .map_err(|e| crate::db::oops("باطل کردن فاکتور نشد", e))?;
        db::audit(
            conn,
            "doc",
            id,
            "void",
            &json!({
                "reason": reason,
                "total": doc.total,
                "released": released,
            })
            .to_string(),
        )?;
        load(conn, id)
    })
}

/// The circled total of the notebook: this invoice is final.
pub fn set_final(conn: &Connection, id: i64, is_final: bool) -> Res<Doc> {
    in_tx(conn, |conn| {
        let doc = load(conn, id)?;
        if doc.status == "void" {
            return Err("فاکتور باطل‌شده را نمی‌شود عوض کرد".to_string());
        }
        conn.execute(
            "UPDATE doc SET final = ?1, updated_at = ?2 WHERE id = ?3",
            params![is_final as i64, db::now_ms(), id],
        )
        .map_err(|e| crate::db::oops("ثبت فاکتور قطعی نشد", e))?;
        db::audit(
            conn,
            "doc",
            id,
            "update",
            &json!({ "final": is_final }).to_string(),
        )?;
        load(conn, id)
    })
}

/// Yesterday's invoice again, for today, at today's prices for this customer.
/// The last price he was charged wins over the catalog: that is what makes an
/// order take seconds instead of a conversation.
pub fn repeat(conn: &Connection, id: i64) -> Res<DocInput> {
    let doc = load(conn, id)?;
    let history_kind = if doc.kind == "sale" { "sale" } else { "buy" };
    let round_step = db::get_setting(conn, "round_step", "0").parse().unwrap_or(0);

    let mut stmt = conn
        .prepare(
            "SELECT product_id, name, qty_milli, unit, price, note FROM doc_line
             WHERE doc_id = ?1 AND voided = 0 ORDER BY pos, id",
        )
        .map_err(|e| crate::db::oops("خواندن سطرها نشد", e))?;
    let raw = stmt
        .query_map([id], |r| {
            Ok((
                r.get::<_, Option<i64>>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|e| crate::db::oops("خواندن سطرها نشد", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| crate::db::oops("خواندن سطرها نشد", e))?;

    let mut lines = Vec::with_capacity(raw.len());
    for (product_id, name, qty_milli, unit, typed_price, note) in raw {
        let price = match product_id {
            Some(pid) => match last_price(conn, pid, doc.party_id, history_kind)? {
                Some(price) => price,
                None => catalog_price(conn, pid, doc.kind.as_str())?.unwrap_or(typed_price),
            },
            None => typed_price,
        };
        lines.push(LineInput {
            id: None,
            product_id,
            name,
            qty_milli,
            unit,
            price,
            override_total: None,
            note,
            voided: false,
        });
    }

    Ok(DocInput {
        id: None,
        kind: doc.kind,
        party_id: doc.party_id,
        day: db::today(),
        discount: 0,
        round_step,
        note: None,
        is_final: false,
        lines,
        received_now: 0,
        received_method: "cash".to_string(),
        update_prices: false,
    })
}

/// Documents newest first. Voided ones are included and marked, because a
/// crossed-out invoice is still part of the book.
pub fn list(
    conn: &Connection,
    kind: Option<&str>,
    party_id: Option<i64>,
    from: Option<i64>,
    to: Option<i64>,
    limit: Option<i64>,
) -> Res<Vec<DocRow>> {
    let limit = limit.unwrap_or(100).clamp(1, 1_000);
    let sql = format!(
        "SELECT {DOC_COLUMNS}, p.name FROM doc d JOIN party p ON p.id = d.party_id
         WHERE (?1 IS NULL OR d.kind = ?1)
           AND (?2 IS NULL OR d.party_id = ?2)
           AND (?3 IS NULL OR d.day >= ?3)
           AND (?4 IS NULL OR d.day <= ?4)
         ORDER BY d.day DESC, d.id DESC LIMIT ?5"
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| crate::db::oops("خواندن فاکتورها نشد", e))?;
    let rows = stmt
        .query_map(params![kind, party_id, from, to, limit], |r| {
            Ok(DocRow {
                doc: from_row(r)?,
                party_name: r.get(16)?,
            })
        })
        .map_err(|e| crate::db::oops("خواندن فاکتورها نشد", e))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| crate::db::oops("خواندن فاکتورها نشد", e))?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// the interface
// ---------------------------------------------------------------------------

fn read<T: serde::de::DeserializeOwned>(args: &Value) -> Res<T> {
    serde_json::from_value(args.clone()).map_err(|_| "فاکتور خوانده نشد".to_string())
}

fn json_of<T: serde::Serialize>(value: T) -> Res<Value> {
    serde_json::to_value(value).map_err(|e| crate::db::oops("آماده کردن پاسخ نشد", e))
}

#[derive(Deserialize)]
struct IdArgs {
    id: i64,
}

#[derive(Deserialize)]
struct VoidArgs {
    id: i64,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Deserialize)]
struct FinalArgs {
    id: i64,
    r#final: bool,
}

/// What the order screen sends. `types.ts` marks the small fields optional —
/// a line's `voided`, a document's `discount` — so a screen that never crosses
/// anything out may leave them out; a missing flag means "no", never a refused
/// save.
fn doc_input(args: &Value) -> Res<DocInput> {
    let mut input = args.get("input").cloned().unwrap_or(Value::Null);
    if let Value::Object(fields) = &mut input {
        if let Some(Value::Array(lines)) = fields.get_mut("lines") {
            for line in lines.iter_mut() {
                if let Value::Object(line) = line {
                    line.entry("voided").or_insert(Value::Bool(false));
                    line.entry("name").or_insert(Value::from(""));
                    line.entry("unit").or_insert(Value::from(""));
                    line.entry("qtyMilli").or_insert(Value::from(0));
                    line.entry("price").or_insert(Value::from(0));
                }
            }
        }
        fields.entry("discount").or_insert(Value::from(0));
        fields.entry("roundStep").or_insert(Value::from(0));
        fields.entry("receivedNow").or_insert(Value::from(0));
        fields.entry("receivedMethod").or_insert(Value::from("cash"));
        fields.entry("updatePrices").or_insert(Value::Bool(false));
    }
    serde_json::from_value(input).map_err(|_| "فاکتور خوانده نشد".to_string())
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ListArgs {
    kind: Option<String>,
    party_id: Option<i64>,
    from: Option<i64>,
    to: Option<i64>,
    limit: Option<i64>,
}

pub fn handle(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    Some(match op {
        "orders.get" => (|| {
            let a: IdArgs = read(args)?;
            let conn = ctx.conn()?;
            json_of(full(&conn, a.id)?)
        })(),
        "orders.save" => (|| {
            let input = doc_input(args)?;
            let conn = ctx.conn()?;
            json_of(save(&conn, &input)?)
        })(),
        "orders.void" => (|| {
            let a: VoidArgs = read(args)?;
            let reason = a.reason.unwrap_or_default();
            let conn = ctx.conn()?;
            json_of(void(&conn, a.id, &reason)?)
        })(),
        "orders.setFinal" => (|| {
            let a: FinalArgs = read(args)?;
            let conn = ctx.conn()?;
            json_of(set_final(&conn, a.id, a.r#final)?)
        })(),
        "orders.repeat" => (|| {
            let a: IdArgs = read(args)?;
            let conn = ctx.conn()?;
            json_of(repeat(&conn, a.id)?)
        })(),
        "orders.list" => (|| {
            let a: ListArgs = read(args)?;
            let conn = ctx.conn()?;
            json_of(list(
                &conn,
                a.kind.as_deref(),
                a.party_id,
                a.from,
                a.to,
                a.limit,
            )?)
        })(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::dto::PartyInput;

    /// 9 Tir 1404 — the page the shopkeeper wrote on 30 June.
    fn tir(day: i64) -> i64 {
        jalali::j2d(1404, 4, day)
    }

    fn party(conn: &Connection, name: &str) -> i64 {
        parties::save(
            conn,
            &PartyInput {
                name: name.to_string(),
                ..Default::default()
            },
        )
        .unwrap()
        .id
    }

    fn line(name: &str, qty_milli: i64, price: i64) -> LineInput {
        LineInput {
            id: None,
            product_id: None,
            name: name.to_string(),
            qty_milli,
            unit: "kg".to_string(),
            price,
            override_total: None,
            note: None,
            voided: false,
        }
    }

    fn sale(party_id: i64, day: i64, lines: Vec<LineInput>) -> DocInput {
        DocInput {
            id: None,
            kind: "sale".to_string(),
            party_id,
            day,
            discount: 0,
            round_step: 0,
            note: None,
            is_final: false,
            lines,
            received_now: 0,
            received_method: "cash".to_string(),
            update_prices: false,
        }
    }

    fn save_doc(conn: &Connection, input: DocInput) -> Doc {
        save(conn, &input).unwrap()
    }

    fn payment(party_id: i64, direction: &str, amount: i64, day: i64) -> PaymentInput {
        PaymentInput {
            id: None,
            party_id,
            direction: direction.to_string(),
            amount,
            day,
            method: "cash".to_string(),
            reference: None,
            note: None,
            allocations: Vec::new(),
        }
    }

    // -- the arithmetic ----------------------------------------------------

    /// The 30 June page, exactly as it is written in the notebook.
    #[test]
    fn the_notebook_page_of_the_thirtieth_of_june_adds_up() {
        let conn = db::memory();
        let id = party(&conn, "لیلا امینی");
        let doc = save_doc(
            &conn,
            sale(
                id,
                tir(9),
                vec![
                    line("لوبیا چیتی", 15_000, 345), // 15 kg × 345
                    line("عدس", 20_000, 285),        // 20 × 285
                    line("برنج", 35_000, 720),       // 35 × 720
                ],
            ),
        );

        let full = full(&conn, doc.id).unwrap();
        assert_eq!(full.lines.len(), 3);
        assert_eq!(full.lines[0].total, 5_175);
        assert_eq!(full.lines[1].total, 5_700);
        assert_eq!(full.lines[2].total, 25_200);
        assert_eq!(full.doc.total, 36_075);
        // and it prints the way the shopkeeper checks it by hand
        assert_eq!(money::fmt_amount(full.doc.total), "۳۶٬۰۷۵");
        assert_eq!(full.doc.remaining, 36_075);
        assert_eq!(full.doc.status, "open");
        assert_eq!(full.doc.jyear, 1404);
        assert_eq!(full.doc.number, 1);
        assert_eq!(balance::of(&conn, id).unwrap(), 36_075);
    }

    #[test]
    fn a_five_line_invoice_round_trips_through_save_and_get() {
        let conn = db::memory();
        let id = party(&conn, "حاج‌آقا");

        let mut lines = vec![
            line("عدس", 2_500, 1_234),  // 3,085
            line("نخود", 1_000, 999),   // 999
            line("لوبیا", 12_750, 88),  // 1,122
            line("کشمش", 500, 12_345),  // 6,172.5 → 6,173
            line("خرما", 3_333, 111),   // 369.963 → 370, typed over as 500
        ];
        lines[4].override_total = Some(500);

        let doc = save_doc(&conn, sale(id, tir(9), lines));
        let full = full(&conn, doc.id).unwrap();

        assert_eq!(full.lines.len(), 5);
        let totals: Vec<i64> = full.lines.iter().map(|l| l.total).collect();
        assert_eq!(totals, vec![3_085, 999, 1_122, 6_173, 500]);
        let positions: Vec<i64> = full.lines.iter().map(|l| l.pos).collect();
        assert_eq!(positions, vec![0, 1, 2, 3, 4]);
        assert_eq!(full.doc.total, 11_879);
        assert_eq!(full.doc.total, totals.iter().sum::<i64>());
        assert_eq!(full.party.name, "حاج‌آقا");
        assert_eq!(full.lines[3].qty_milli, 500);
        assert_eq!(full.lines[4].override_total, Some(500));
    }

    #[test]
    fn discount_and_rounding_round_the_total_once() {
        let conn = db::memory();
        let id = party(&conn, "لیلا امینی");

        let mut input = sale(
            id,
            tir(9),
            vec![line("لوبیا چیتی", 15_000, 345), line("عدس", 20_000, 285)],
        );
        input.discount = 1_000;
        input.round_step = 100;
        let doc = save_doc(&conn, input);

        // 5,175 + 5,700 = 10,875, less the 1,000 discount = 9,875,
        // rounded once, on the total, to the nearest hundred.
        assert_eq!(doc.total, 9_900);
        assert_eq!(doc.rounding, 25);
        assert_eq!(doc.discount, 1_000);
        assert_eq!(doc.remaining, 9_900);
        // the invoice adds up in public: lines − discount + rounding = total
        assert_eq!(10_875 - doc.discount + doc.rounding, doc.total);

        // with no rounding step the discount is the whole story
        let mut plain = sale(
            id,
            tir(10),
            vec![line("لوبیا چیتی", 15_000, 345), line("عدس", 20_000, 285)],
        );
        plain.discount = 1_000;
        let plain = save_doc(&conn, plain);
        assert_eq!(plain.total, 9_875);
        assert_eq!(plain.rounding, 0);
    }

    // -- money -----------------------------------------------------------------

    #[test]
    fn a_partial_payment_leaves_the_exact_remaining() {
        let conn = db::memory();
        let id = party(&conn, "لیلا امینی");
        let doc = save_doc(
            &conn,
            sale(
                id,
                tir(9),
                vec![
                    line("لوبیا چیتی", 15_000, 345),
                    line("عدس", 20_000, 285),
                    line("برنج", 35_000, 720),
                ],
            ),
        );

        payments::save(&conn, &payment(id, "in", 10_000, tir(10))).unwrap();
        let half = load(&conn, doc.id).unwrap();
        assert_eq!(half.paid, 10_000);
        assert_eq!(half.remaining, 26_075);
        assert_eq!(half.status, "open");
        assert_eq!(balance::of(&conn, id).unwrap(), 26_075);

        let last = payments::save(&conn, &payment(id, "in", 26_075, tir(11))).unwrap();
        assert_eq!(last.amount, 26_075);
        let done = load(&conn, doc.id).unwrap();
        assert_eq!(done.paid, 36_075);
        assert_eq!(done.remaining, 0);
        assert_eq!(done.status, "settled");
        assert_eq!(balance::of(&conn, id).unwrap(), 0);
    }

    #[test]
    fn an_overpayment_becomes_credit_on_the_party() {
        let conn = db::memory();
        let id = party(&conn, "لیلا امینی");
        let doc = save_doc(
            &conn,
            sale(id, tir(9), vec![line("لوبیا چیتی", 15_000, 345)]),
        );
        assert_eq!(doc.total, 5_175);

        let paid = payments::save(&conn, &payment(id, "in", 6_000, tir(9))).unwrap();
        assert_eq!(payments::allocated_of(&conn, paid.id).unwrap(), 5_175);

        let rows = payments::list(&conn, Some(id), None, None, None).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].allocated, 5_175);
        assert_eq!(rows[0].unallocated, 825);

        // The 825 he handed over stays on his side of the book as credit:
        // the shop owes it back, so the balance is 825 below zero and no lower.
        assert_eq!(balance::of(&conn, id).unwrap(), -825);
        assert_eq!(load(&conn, doc.id).unwrap().status, "settled");
        assert_eq!(load(&conn, doc.id).unwrap().remaining, 0);
    }

    #[test]
    fn voiding_a_document_and_a_payment_gives_the_balance_back() {
        // a payment crossed out
        let conn = db::memory();
        let id = party(&conn, "لیلا امینی");
        let doc = save_doc(
            &conn,
            sale(id, tir(9), vec![line("لوبیا چیتی", 15_000, 345)]),
        );
        assert_eq!(balance::of(&conn, id).unwrap(), 5_175);

        let paid = payments::save(&conn, &payment(id, "in", 2_000, tir(9))).unwrap();
        assert_eq!(balance::of(&conn, id).unwrap(), 3_175);
        assert_eq!(load(&conn, doc.id).unwrap().paid, 2_000);

        payments::void(&conn, paid.id, "اشتباه ثبت شد").unwrap();
        assert_eq!(balance::of(&conn, id).unwrap(), 5_175);
        assert_eq!(load(&conn, doc.id).unwrap().paid, 0);
        assert_eq!(load(&conn, doc.id).unwrap().status, "open");

        // a document crossed out
        assert!(void(&conn, doc.id, "   ").is_err(), "دلیل لازم است");
        void(&conn, doc.id, "فاکتور غلط بود").unwrap();
        assert_eq!(load(&conn, doc.id).unwrap().status, "void");
        assert_eq!(balance::of(&conn, id).unwrap(), 0);
        assert_eq!(lines_of(&conn, doc.id).unwrap().len(), 1);

        // money paid against an invoice that was then voided comes back as
        // credit; it must not disappear with the invoice.
        let conn = db::memory();
        let id = party(&conn, "لیلا امینی");
        let doc = save_doc(
            &conn,
            sale(id, tir(9), vec![line("لوبیا چیتی", 15_000, 345)]),
        );
        payments::save(&conn, &payment(id, "in", 2_000, tir(9))).unwrap();
        void(&conn, doc.id, "فاکتور غلط بود").unwrap();
        assert_eq!(balance::of(&conn, id).unwrap(), -2_000);
    }

    #[test]
    fn cash_at_the_counter_is_recorded_as_a_payment() {
        let conn = db::memory();
        let id = party(&conn, "لیلا امینی");
        let mut input = sale(
            id,
            tir(9),
            vec![line("لوبیا چیتی", 15_000, 345), line("عدس", 20_000, 285)],
        );
        input.received_now = 3_000;
        let doc = save_doc(&conn, input);

        assert_eq!(doc.total, 10_875);
        assert_eq!(doc.paid, 3_000);
        assert_eq!(doc.remaining, 7_875);
        assert_eq!(doc.status, "open");

        let rows = payments::list(&conn, Some(id), None, None, None).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].payment.direction, "in");
        assert_eq!(rows[0].payment.amount, 3_000);
        assert_eq!(rows[0].allocated, 3_000);
        assert_eq!(rows[0].unallocated, 0);
    }

    // -- prices, numbers, the book --------------------------------------------

    #[test]
    fn repeat_brings_back_this_customers_last_price_not_the_catalog() {
        let conn = db::memory();
        conn.execute(
            "INSERT INTO product(id, name, unit, sale_price, buy_price, created_at, updated_at)
             VALUES(1, 'عدس', 'kg', 400, 300, 0, 0)",
            [],
        )
        .unwrap();
        let id = party(&conn, "لیلا امینی");

        let mut first = line("عدس", 10_000, 345);
        first.product_id = Some(1);
        let mut input = sale(id, tir(9), vec![first]);
        input.update_prices = true;
        let doc = save_doc(&conn, input);
        assert_eq!(
            catalog_price(&conn, 1, "sale").unwrap(),
            Some(345),
            "the catalog now says 345"
        );

        // somebody else buys at the source's new price, so the catalog moves
        let other = party(&conn, "رضا");
        let mut second = line("عدس", 5_000, 400);
        second.product_id = Some(1);
        let mut later = sale(other, tir(10), vec![second]);
        later.update_prices = true;
        save_doc(&conn, later);
        assert_eq!(catalog_price(&conn, 1, "sale").unwrap(), Some(400));

        // but this customer's own last price is what he is charged again
        let again = repeat(&conn, doc.id).unwrap();
        assert_eq!(again.lines.len(), 1);
        assert_eq!(again.lines[0].price, 345);
        assert_eq!(again.lines[0].name, "عدس");
        assert_eq!(again.id, None);
        assert_eq!(again.day, db::today());
    }

    #[test]
    fn numbers_are_sequential_per_jalali_year_and_never_reused() {
        let conn = db::memory();
        let id = party(&conn, "لیلا امینی");
        let y1403 = jalali::j2d(1403, 12, 20);
        let y1404 = jalali::j2d(1404, 1, 5);

        let a = save_doc(&conn, sale(id, y1403, vec![line("عدس", 1_000, 100)]));
        assert_eq!((a.jyear, a.number), (1403, 1));

        let b = save_doc(&conn, sale(id, y1404, vec![line("عدس", 1_000, 100)]));
        assert_eq!((b.jyear, b.number), (1404, 1), "a new year starts at one");

        let c = save_doc(&conn, sale(id, y1403, vec![line("عدس", 1_000, 100)]));
        assert_eq!((c.jyear, c.number), (1403, 2));

        // crossing out number 1 does not hand number 1 out again
        void(&conn, a.id, "غلط بود").unwrap();
        let d = save_doc(&conn, sale(id, y1403, vec![line("عدس", 1_000, 100)]));
        assert_eq!(d.number, 3);

        // an edit keeps the number it was given
        let mut edit = sale(id, y1403, vec![line("عدس", 2_000, 100)]);
        edit.id = Some(c.id);
        let edited = save_doc(&conn, edit);
        assert_eq!(edited.number, 2);
        assert_eq!(edited.total, 200);

        // purchases keep their own series
        let mut purchase = sale(id, y1404, vec![line("عدس", 1_000, 100)]);
        purchase.kind = "purchase".to_string();
        let bought = save_doc(&conn, purchase);
        assert_eq!((bought.jyear, bought.number), (1404, 1));
    }

    #[test]
    fn every_save_leaves_an_audit_row() {
        let conn = db::memory();
        let id = party(&conn, "لیلا امینی");
        let doc = save_doc(
            &conn,
            sale(id, tir(9), vec![line("لوبیا چیتی", 15_000, 345)]),
        );
        let mut edit = sale(id, tir(9), vec![line("لوبیا چیتی", 16_000, 345)]);
        edit.id = Some(doc.id);
        save_doc(&conn, edit);
        void(&conn, doc.id, "غلط بود").unwrap();

        let mut stmt = conn
            .prepare("SELECT action FROM audit WHERE entity = 'doc' AND entity_id = ?1 ORDER BY id")
            .unwrap();
        let actions = stmt
            .query_map([doc.id], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(actions, vec!["create", "update", "void"]);
    }

    #[test]
    fn the_order_interface_answers_its_own_names() {
        let state = memory_state();
        let ctx = Ctx { state: &state };
        let call = |op: &str, args: Value| handle(&ctx, op, &args).unwrap().unwrap();
        let id = party(&state.db.lock().unwrap(), "لیلا امینی");

        assert!(handle(&ctx, "orders.archive", &json!({})).is_none());
        assert!(handle(&ctx, "parties.list", &json!({})).is_none());

        // the interface sends camelCase, and `final` is spelled the way the
        // notebook says it
        let saved: Doc = serde_json::from_value(call(
            "orders.save",
            json!({"input": {
                "kind": "sale",
                "partyId": id,
                "day": tir(9),
                "discount": 0,
                "roundStep": 0,
                "final": false,
                "lines": [{"name": "لوبیا چیتی", "qtyMilli": 15_000, "unit": "kg", "price": 345}],
                "receivedNow": 0,
                "receivedMethod": "cash",
                "updatePrices": false
            }}),
        ))
        .unwrap();
        assert_eq!(saved.total, 5_175);
        assert!(!saved.is_final);

        let page: DocFull = serde_json::from_value(call("orders.get", json!({"id": saved.id}))).unwrap();
        assert_eq!(page.lines.len(), 1);
        assert_eq!(page.party.name, "لیلا امینی");

        let listed: Vec<DocRow> =
            serde_json::from_value(call("orders.list", json!({"partyId": id, "kind": "sale"}))).unwrap();
        assert_eq!(listed.len(), 1);

        let again: DocInput = serde_json::from_value(call("orders.repeat", json!({"id": saved.id}))).unwrap();
        assert_eq!(again.lines.len(), 1);
        assert_eq!(again.kind, "sale");

        let circled: Doc =
            serde_json::from_value(call("orders.setFinal", json!({"id": saved.id, "final": true}))).unwrap();
        assert!(circled.is_final);

        let crossed: Doc =
            serde_json::from_value(call("orders.void", json!({"id": saved.id, "reason": "غلط بود"}))).unwrap();
        assert_eq!(crossed.status, "void");

        // no reason, no crossing out — and the sentence speaks the way he does
        assert_eq!(
            handle(&ctx, "orders.void", &json!({"id": saved.id, "reason": "  "}))
                .unwrap()
                .unwrap_err(),
            "دلیلش را بنویس"
        );
    }

    fn memory_state() -> crate::AppState {
        crate::AppState {
            db: std::sync::Mutex::new(db::memory()),
            dirs: crate::Dirs {
                data: std::path::PathBuf::new(),
                files: std::path::PathBuf::new(),
            },
        }
    }

    #[test]
    fn crossed_out_lines_stay_readable_and_stop_counting() {
        let conn = db::memory();
        let id = party(&conn, "لیلا امینی");
        let doc = save_doc(
            &conn,
            sale(
                id,
                tir(9),
                vec![line("لوبیا چیتی", 15_000, 345), line("عدس", 20_000, 285)],
            ),
        );
        assert_eq!(doc.total, 10_875);

        // the shopkeeper crosses out the second line on an edit
        let kept = full(&conn, doc.id).unwrap().lines[0].id;
        let mut input = sale(id, tir(9), vec![line("لوبیا چیتی", 15_000, 345)]);
        input.id = Some(doc.id);
        input.lines[0].id = Some(kept);
        let edited = save_doc(&conn, input);

        assert_eq!(edited.total, 5_175);
        let lines = lines_of(&conn, doc.id).unwrap();
        assert_eq!(lines.len(), 2, "the crossed-out line is still on the page");
        assert_eq!(lines[1].name, "عدس");
        assert!(lines[1].voided);
    }
}
