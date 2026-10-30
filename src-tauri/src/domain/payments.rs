//! Payments, their allocation over invoices, and settlement.
//!
//! A payment is money that actually moved. What it was *for* is an allocation:
//! one payment can settle three invoices, and whatever is left over is credit on
//! the party rather than a discount. The oldest debt is always the one that gets
//! paid first, which is how a shopkeeper works down a page.

use crate::db::{self, Res};
use crate::domain::orders::{self, in_tx, refresh_status};
use crate::domain::parties;
use crate::dto::{AllocationInput, Payment, PaymentInput, PaymentRow};
use crate::Ctx;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

pub fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Payment> {
    Ok(Payment {
        id: r.get(0)?,
        party_id: r.get(1)?,
        direction: r.get(2)?,
        amount: r.get(3)?,
        day: r.get(4)?,
        method: r.get(5)?,
        reference: r.get(6)?,
        note: r.get(7)?,
        voided: r.get::<_, i64>(8)? != 0,
        void_reason: r.get(9)?,
        created_at: r.get(10)?,
    })
}

pub const PAYMENT_COLUMNS: &str =
    "p.id, p.party_id, p.direction, p.amount, p.day, p.method, p.ref, p.note,
      p.voided, p.void_reason, p.created_at";

pub fn load(conn: &Connection, id: i64) -> Res<Payment> {
    conn.query_row(
        &format!("SELECT {PAYMENT_COLUMNS} FROM payment p WHERE p.id = ?1"),
        [id],
        from_row,
    )
    .optional()
    .map_err(|e| format!("خواندن پرداخت نشد: {e}"))?
    .ok_or_else(|| "پرداخت پیدا نشد".to_string())
}

/// How much of a payment was applied to invoices.
pub fn allocated_of(conn: &Connection, payment_id: i64) -> Res<i64> {
    conn.query_row(
        "SELECT COALESCE(SUM(amount), 0) FROM allocation WHERE payment_id = ?1",
        [payment_id],
        |r| r.get(0),
    )
    .map_err(|e| format!("محاسبه پرداخت نشد: {e}"))
}

fn allocated_docs(conn: &Connection, payment_id: i64) -> Res<Vec<i64>> {
    let mut stmt = conn
        .prepare("SELECT DISTINCT doc_id FROM allocation WHERE payment_id = ?1")
        .map_err(|e| format!("خواندن پرداخت‌ها نشد: {e}"))?;
    let rows = stmt
        .query_map([payment_id], |r| r.get::<_, i64>(0))
        .map_err(|e| format!("خواندن پرداخت‌ها نشد: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("خواندن پرداخت‌ها نشد: {e}"))?);
    }
    Ok(out)
}

/// The oldest-first spread over everything this party still owes of that kind.
/// Anything the amount cannot cover stays in the payment as credit.
fn spread(conn: &Connection, party_id: i64, kind: &str, amount: i64) -> Res<Vec<AllocationInput>> {
    if amount <= 0 {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare(
            "SELECT id FROM doc
             WHERE party_id = ?1 AND kind = ?2 AND status <> 'void'
             ORDER BY day ASC, id ASC",
        )
        .map_err(|e| format!("خواندن فاکتورها نشد: {e}"))?;
    let ids = stmt
        .query_map(params![party_id, kind], |r| r.get::<_, i64>(0))
        .map_err(|e| format!("خواندن فاکتورها نشد: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("خواندن فاکتورها نشد: {e}"))?;

    let mut left = amount;
    let mut out = Vec::new();
    for doc_id in ids {
        if left <= 0 {
            break;
        }
        let doc = orders::load(conn, doc_id)?;
        if doc.remaining <= 0 {
            continue;
        }
        let take = left.min(doc.remaining);
        out.push(AllocationInput {
            doc_id,
            amount: take,
        });
        left -= take;
    }
    Ok(out)
}

/// Allocations the screen typed by hand, checked against what each invoice
/// still has on it. A payment may not invent money against an invoice.
fn checked_plan(
    conn: &Connection,
    input: &PaymentInput,
    kind: &str,
) -> Res<Vec<AllocationInput>> {
    let mut per_doc: HashMap<i64, i64> = HashMap::new();
    let mut total = 0i64;
    let mut out = Vec::new();
    for allocation in &input.allocations {
        if allocation.amount < 0 {
            return Err("مبلغ را درست وارد کنید".to_string());
        }
        if allocation.amount == 0 {
            continue;
        }
        let doc = orders::load(conn, allocation.doc_id)?;
        if doc.party_id != input.party_id {
            return Err("این فاکتور مال همین طرف‌حساب نیست".to_string());
        }
        if doc.kind != kind {
            return Err("این فاکتور با این پرداخت جور نیست".to_string());
        }
        if doc.status == "void" {
            return Err("فاکتور باطل‌شده را نمی‌شود پرداخت کرد".to_string());
        }
        let running = per_doc.entry(allocation.doc_id).or_insert(0);
        *running += allocation.amount;
        if *running > doc.remaining {
            return Err("این مبلغ از مانده فاکتور بیشتر است".to_string());
        }
        total += allocation.amount;
        out.push(allocation.clone());
    }
    if total > input.amount {
        return Err("بیشتر از مبلغ پرداخت نمی‌شود به فاکتور بست".to_string());
    }
    Ok(out)
}

/// Records a payment inside the caller's transaction. `auto` spreads the amount
/// over the oldest unpaid invoices when no allocation was given; the order
/// screen passes `false` because the cash it hands over belongs to one invoice
/// and anything extra is credit.
pub fn record(conn: &Connection, input: &PaymentInput, auto: bool) -> Res<Payment> {
    if input.amount <= 0 {
        return Err("مبلغ را درست وارد کنید".to_string());
    }
    let direction = match input.direction.as_str() {
        "in" => "in",
        "out" => "out",
        _ => return Err("نوع پرداخت نامشخص است".to_string()),
    };
    let day = if input.day > 0 { input.day } else { db::today() };
    let method = match input.method.trim() {
        "" => "cash".to_string(),
        other => other.to_string(),
    };
    let note = input
        .note
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let reference = input
        .reference
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    parties::party_of(conn, input.party_id)?;

    let editing = input.id.filter(|id| *id > 0);
    let mut touched: HashSet<i64> = HashSet::new();
    let id = match editing {
        Some(id) => {
            let old = load(conn, id)?;
            if old.voided {
                return Err("این پرداخت باطل شده و عوض نمی‌شود".to_string());
            }
            if old.party_id != input.party_id {
                return Err("این پرداخت مال همین طرف‌حساب نیست".to_string());
            }
            for doc_id in allocated_docs(conn, id)? {
                touched.insert(doc_id);
            }
            conn.execute(
                "UPDATE payment SET party_id = ?1, direction = ?2, amount = ?3, day = ?4,
                        method = ?5, ref = ?6, note = ?7
                 WHERE id = ?8",
                params![
                    input.party_id,
                    direction,
                    input.amount,
                    day,
                    method,
                    reference,
                    note,
                    id
                ],
            )
            .map_err(|e| format!("ذخیره پرداخت نشد: {e}"))?;
            conn.execute("DELETE FROM allocation WHERE payment_id = ?1", [id])
                .map_err(|e| format!("ذخیره پرداخت نشد: {e}"))?;
            id
        }
        None => {
            conn.execute(
                "INSERT INTO payment(party_id, direction, amount, day, method, ref, note,
                                     voided, created_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
                params![
                    input.party_id,
                    direction,
                    input.amount,
                    day,
                    method,
                    reference,
                    note,
                    db::now_ms()
                ],
            )
            .map_err(|e| format!("ذخیره پرداخت نشد: {e}"))?;
            conn.last_insert_rowid()
        }
    };

    let kind = if direction == "in" { "sale" } else { "purchase" };
    let plan = if input.allocations.is_empty() {
        if auto {
            spread(conn, input.party_id, kind, input.amount)?
        } else {
            Vec::new()
        }
    } else {
        checked_plan(conn, input, kind)?
    };

    let mut allocated = 0i64;
    for allocation in &plan {
        conn.execute(
            "INSERT INTO allocation(payment_id, doc_id, amount) VALUES(?1, ?2, ?3)",
            params![id, allocation.doc_id, allocation.amount],
        )
        .map_err(|e| format!("نسبت دادن پرداخت نشد: {e}"))?;
        allocated += allocation.amount;
        touched.insert(allocation.doc_id);
    }

    for doc_id in touched {
        refresh_status(conn, doc_id)?;
    }

    let action = if editing.is_some() { "update" } else { "create" };
    db::audit(
        conn,
        "payment",
        id,
        action,
        &json!({
            "partyId": input.party_id,
            "direction": direction,
            "amount": input.amount,
            "day": day,
            "allocated": allocated,
            "unallocated": input.amount - allocated,
        })
        .to_string(),
    )?;

    load(conn, id)
}

pub fn save(conn: &Connection, input: &PaymentInput) -> Res<Payment> {
    in_tx(conn, |conn| record(conn, input, true))
}

/// A voided payment stops counting towards anything. Its allocations are kept:
/// the paper keeps the record, the balance does not believe it.
pub fn void(conn: &Connection, id: i64, reason: &str) -> Res<Payment> {
    let reason = reason.trim().to_string();
    if reason.is_empty() {
        return Err("دلیلش را بنویسید".to_string());
    }
    in_tx(conn, |conn| {
        let payment = load(conn, id)?;
        if payment.voided {
            return Err("این پرداخت قبلاً باطل شده".to_string());
        }
        let touched = allocated_docs(conn, id)?;
        conn.execute(
            "UPDATE payment SET voided = 1, void_reason = ?1 WHERE id = ?2",
            params![reason, id],
        )
        .map_err(|e| format!("باطل کردن پرداخت نشد: {e}"))?;
        for doc_id in touched {
            refresh_status(conn, doc_id)?;
        }
        db::audit(
            conn,
            "payment",
            id,
            "void",
            &json!({
                "reason": reason,
                "amount": payment.amount,
                "partyId": payment.party_id,
            })
            .to_string(),
        )?;
        load(conn, id)
    })
}

/// Payments newest first, with the part of each one that landed on an invoice.
pub fn list(
    conn: &Connection,
    party_id: Option<i64>,
    from: Option<i64>,
    to: Option<i64>,
    limit: Option<i64>,
) -> Res<Vec<PaymentRow>> {
    let limit = limit.unwrap_or(100).clamp(1, 1_000);
    let sql = format!(
        "SELECT {PAYMENT_COLUMNS}, pt.name,
                COALESCE((SELECT SUM(a.amount) FROM allocation a
                          WHERE a.payment_id = p.id), 0)
         FROM payment p JOIN party pt ON pt.id = p.party_id
         WHERE (?1 IS NULL OR p.party_id = ?1)
           AND (?2 IS NULL OR p.day >= ?2)
           AND (?3 IS NULL OR p.day <= ?3)
         ORDER BY p.day DESC, p.id DESC LIMIT ?4"
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("خواندن پرداخت‌ها نشد: {e}"))?;
    let rows = stmt
        .query_map(params![party_id, from, to, limit], |r| {
            let payment = from_row(r)?;
            let allocated: i64 = r.get(12)?;
            Ok(PaymentRow {
                party_name: r.get(11)?,
                allocated,
                unallocated: payment.amount - allocated,
                payment,
            })
        })
        .map_err(|e| format!("خواندن پرداخت‌ها نشد: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("خواندن پرداخت‌ها نشد: {e}"))?);
    }
    Ok(out)
}

/// What will happen if this amount is saved, so the screen can promise it before
/// it happens. A party who owes me is being paid (money in, against sales); a
/// party I owe is being paid out, against purchases.
pub fn suggest(conn: &Connection, party_id: i64, amount: i64) -> Res<Vec<AllocationInput>> {
    if amount <= 0 {
        return Ok(Vec::new());
    }
    parties::party_of(conn, party_id)?;
    let balance = crate::domain::balance::of(conn, party_id)?;
    let kind = if balance >= 0 { "sale" } else { "purchase" };
    spread(conn, party_id, kind, amount)
}

// ---------------------------------------------------------------------------
// the interface
// ---------------------------------------------------------------------------

fn read<T: serde::de::DeserializeOwned>(args: &Value) -> Res<T> {
    serde_json::from_value(args.clone()).map_err(|_| "ورودی خوانده نشد".to_string())
}

fn json_of<T: serde::Serialize>(value: T) -> Res<Value> {
    serde_json::to_value(value).map_err(|e| format!("آماده کردن پاسخ نشد: {e}"))
}

#[derive(Deserialize)]
struct SaveArgs {
    input: PaymentInput,
}

#[derive(Deserialize)]
struct VoidArgs {
    id: i64,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ListArgs {
    party_id: Option<i64>,
    from: Option<i64>,
    to: Option<i64>,
    limit: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SuggestArgs {
    party_id: i64,
    amount: i64,
}

pub fn handle(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    Some(match op {
        "payments.save" => (|| {
            let a: SaveArgs = read(args)?;
            let conn = ctx.conn()?;
            json_of(save(&conn, &a.input)?)
        })(),
        "payments.void" => (|| {
            let a: VoidArgs = read(args)?;
            let reason = a.reason.unwrap_or_default();
            let conn = ctx.conn()?;
            json_of(void(&conn, a.id, &reason)?)
        })(),
        "payments.list" => (|| {
            let a: ListArgs = read(args)?;
            let conn = ctx.conn()?;
            json_of(list(&conn, a.party_id, a.from, a.to, a.limit)?)
        })(),
        "payments.suggest" => (|| {
            let a: SuggestArgs = read(args)?;
            let conn = ctx.conn()?;
            json_of(suggest(&conn, a.party_id, a.amount)?)
        })(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::domain::balance;
    use crate::dto::{DocInput, LineInput, PartyInput};
    use crate::jalali;

    fn day(n: i64) -> i64 {
        jalali::j2d(1404, 4, n)
    }

    fn add(conn: &Connection, name: &str) -> i64 {
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

    fn doc(conn: &Connection, party_id: i64, kind: &str, on: i64, qty: i64, price: i64) -> i64 {
        orders::save(
            conn,
            &DocInput {
                kind: kind.to_string(),
                party_id,
                day: on,
                lines: vec![LineInput {
                    name: "عدس".to_string(),
                    qty_milli: qty,
                    unit: "kg".to_string(),
                    price,
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
        .unwrap()
        .id
    }

    fn payment(party_id: i64, direction: &str, amount: i64, on: i64) -> PaymentInput {
        PaymentInput {
            party_id,
            direction: direction.to_string(),
            amount,
            day: on,
            method: "cash".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn an_allocation_cannot_be_more_than_the_invoice_has_left() {
        let conn = db::memory();
        let id = add(&conn, "لیلا امینی");
        let invoice = doc(&conn, id, "sale", day(9), 15_000, 345); // 5,175

        let too_much = PaymentInput {
            allocations: vec![AllocationInput {
                doc_id: invoice,
                amount: 6_000,
            }],
            ..payment(id, "in", 6_000, day(10))
        };
        assert_eq!(
            save(&conn, &too_much).unwrap_err(),
            "این مبلغ از مانده فاکتور بیشتر است".to_string()
        );

        // a real part payment
        let part = PaymentInput {
            allocations: vec![AllocationInput {
                doc_id: invoice,
                amount: 3_000,
            }],
            ..payment(id, "in", 3_000, day(10))
        };
        save(&conn, &part).unwrap();
        assert_eq!(orders::load(&conn, invoice).unwrap().remaining, 2_175);

        // now only 2,175 is left, so 5,000 on it is refused
        let still_too_much = PaymentInput {
            allocations: vec![AllocationInput {
                doc_id: invoice,
                amount: 5_000,
            }],
            ..payment(id, "in", 5_000, day(11))
        };
        assert_eq!(
            save(&conn, &still_too_much).unwrap_err(),
            "این مبلغ از مانده فاکتور بیشتر است".to_string()
        );

        // an amount below the allocations is refused too: money cannot be made up
        let over_allocated = PaymentInput {
            allocations: vec![AllocationInput {
                doc_id: invoice,
                amount: 2_175,
            }],
            ..payment(id, "in", 1_000, day(11))
        };
        assert_eq!(
            save(&conn, &over_allocated).unwrap_err(),
            "بیشتر از مبلغ پرداخت نمی‌شود به فاکتور بست".to_string()
        );

        // money paid out does not pay a sale
        let wrong_way = PaymentInput {
            allocations: vec![AllocationInput {
                doc_id: invoice,
                amount: 1_000,
            }],
            ..payment(id, "out", 1_000, day(11))
        };
        assert_eq!(
            save(&conn, &wrong_way).unwrap_err(),
            "این فاکتور با این پرداخت جور نیست".to_string()
        );

        // and one party's money does not pay another party's invoice
        let someone = add(&conn, "رضا");
        let elsewhere = PaymentInput {
            allocations: vec![AllocationInput {
                doc_id: invoice,
                amount: 1_000,
            }],
            ..payment(someone, "in", 1_000, day(11))
        };
        assert_eq!(
            save(&conn, &elsewhere).unwrap_err(),
            "این فاکتور مال همین طرف‌حساب نیست".to_string()
        );

        assert!(save(&conn, &payment(id, "in", 0, day(11))).is_err());
    }

    #[test]
    fn suggest_shows_the_oldest_first_spread() {
        let conn = db::memory();
        let id = add(&conn, "لیلا امینی");
        let first = doc(&conn, id, "sale", day(1), 1_000, 1_000); // 1,000
        let second = doc(&conn, id, "sale", day(5), 2_000, 1_000); // 2,000
        let third = doc(&conn, id, "sale", day(9), 3_000, 1_000); // 3,000

        let plan = suggest(&conn, id, 3_500).unwrap();
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[0].doc_id, first);
        assert_eq!(plan[0].amount, 1_000);
        assert_eq!(plan[1].doc_id, second);
        assert_eq!(plan[1].amount, 2_000);
        assert_eq!(plan[2].doc_id, third);
        assert_eq!(plan[2].amount, 500);

        // and what it promised is what saving does
        let saved = save(&conn, &payment(id, "in", 3_500, day(10))).unwrap();
        assert_eq!(allocated_of(&conn, saved.id).unwrap(), 3_500);
        assert_eq!(orders::load(&conn, first).unwrap().status, "settled");
        assert_eq!(orders::load(&conn, third).unwrap().remaining, 2_500);

        assert!(suggest(&conn, id, 0).unwrap().is_empty());
    }

    #[test]
    fn money_paid_to_a_supplier_settles_purchases() {
        let conn = db::memory();
        let supplier = add(&conn, "حاج‌آقا");
        let bought = doc(&conn, supplier, "purchase", day(9), 40_000, 1_000);
        assert_eq!(balance::of(&conn, supplier).unwrap(), -40_000);

        let first = save(&conn, &payment(supplier, "out", 15_000, day(10))).unwrap();
        assert_eq!(allocated_of(&conn, first.id).unwrap(), 15_000);
        assert_eq!(orders::load(&conn, bought).unwrap().paid, 15_000);
        assert_eq!(orders::load(&conn, bought).unwrap().status, "open");
        assert_eq!(balance::of(&conn, supplier).unwrap(), -25_000);

        save(&conn, &payment(supplier, "out", 25_000, day(11))).unwrap();
        assert_eq!(orders::load(&conn, bought).unwrap().status, "settled");
        assert_eq!(balance::of(&conn, supplier).unwrap(), 0);
    }

    #[test]
    fn the_payment_interface_answers_its_own_names() {
        let state = memory_state();
        let ctx = Ctx { state: &state };
        let call = |op: &str, args: Value| handle(&ctx, op, &args).unwrap().unwrap();

        assert!(handle(&ctx, "payments.pay", &json!({})).is_none());
        assert!(handle(&ctx, "orders.void", &json!({})).is_none());

        let (id, invoice) = {
            let conn = state.db.lock().unwrap();
            let id = add(&conn, "لیلا امینی");
            (id, doc(&conn, id, "sale", day(9), 15_000, 345))
        };

        // the interface sends camelCase and an empty allocation list
        let saved: Payment = serde_json::from_value(call(
            "payments.save",
            json!({"input": {
                "partyId": id,
                "direction": "in",
                "amount": 2_000,
                "day": day(10),
                "method": "cash",
                "allocations": []
            }}),
        ))
        .unwrap();
        assert_eq!(saved.amount, 2_000);
        assert_eq!(saved.direction, "in");

        let plan: Vec<AllocationInput> =
            serde_json::from_value(call("payments.suggest", json!({"partyId": id, "amount": 3_175})))
                .unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].doc_id, invoice);
        assert_eq!(plan[0].amount, 3_175);

        let rows: Vec<PaymentRow> =
            serde_json::from_value(call("payments.list", json!({"partyId": id}))).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].allocated, 2_000);
        assert_eq!(rows[0].unallocated, 0);
        assert_eq!(rows[0].party_name, "لیلا امینی");

        let crossed: Payment = serde_json::from_value(call(
            "payments.void",
            json!({"id": saved.id, "reason": "اشتباه ثبت شد"}),
        ))
        .unwrap();
        assert!(crossed.voided);

        assert_eq!(
            handle(&ctx, "payments.void", &json!({"id": saved.id, "reason": ""}))
                .unwrap()
                .unwrap_err(),
            "دلیلش را بنویسید"
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
}
