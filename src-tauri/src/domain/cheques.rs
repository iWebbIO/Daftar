//! Cheques: the paper slip that only becomes money when it clears.
//!
//! A cheque is not a payment. It becomes one at the single moment it clears,
//! and every status change goes through [`set_status`] so that moment happens
//! once. Clearing a cheque twice must never move a balance twice; bouncing one
//! puts the money back exactly where it was.

use crate::db::{self, Res};
use crate::domain::{orders, payments};
use crate::dto::{Cheque, ChequeInput, ChequeRow, PaymentInput};
use crate::Ctx;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

/// Everything a cheque needs to be read, followed by the party's name.
const COLUMNS: &str = "c.id, c.party_id, c.direction, c.number, c.bank, c.amount, c.due_day, \
                       c.status, c.note, c.payment_id, c.created_at, p.name";

pub const STATUSES: [&str; 4] = ["pending", "cleared", "bounced", "returned"];

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ChequeRow> {
    Ok(ChequeRow {
        cheque: Cheque {
            id: r.get(0)?,
            party_id: r.get(1)?,
            direction: r.get(2)?,
            number: r.get(3)?,
            bank: r.get(4)?,
            amount: r.get(5)?,
            due_day: r.get(6)?,
            status: r.get(7)?,
            note: r.get(8)?,
            payment_id: r.get(9)?,
            created_at: r.get(10)?,
        },
        party_name: r.get(11)?,
    })
}

/// One cheque with the party's name, or a Persian sentence saying it is gone.
pub fn get(conn: &Connection, id: i64) -> Res<ChequeRow> {
    conn.query_row(
        &format!(
            "SELECT {COLUMNS} FROM cheque c JOIN party p ON p.id = c.party_id WHERE c.id = ?1"
        ),
        [id],
        from_row,
    )
    .optional()
    .map_err(|_| "دفتر خوانده نشد".to_string())?
    .ok_or_else(|| "چک پیدا نشد".to_string())
}

/// Pending cheques first, soonest due at the top; everything settled after it,
/// newest first. That is the order the shopkeeper needs: what is coming, then
/// what already happened.
pub fn list(conn: &Connection, status: Option<&str>) -> Res<Vec<ChequeRow>> {
    let wanted = status.map(str::trim).filter(|s| !s.is_empty());
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM cheque c JOIN party p ON p.id = c.party_id
             WHERE (?1 IS NULL OR c.status = ?1)
             ORDER BY CASE WHEN c.status = 'pending' THEN 0 ELSE 1 END,
                      CASE WHEN c.status = 'pending' THEN c.due_day END ASC,
                      c.due_day DESC, c.id DESC"
        ))
        .map_err(|_| "دفتر خوانده نشد".to_string())?;
    let rows = stmt
        .query_map([wanted], from_row)
        .map_err(|_| "دفتر خوانده نشد".to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "دفتر خوانده نشد".to_string())?;
    Ok(rows)
}

/// `None` keeps what was there, an empty string clears the field.
fn text(given: Option<&str>, old: Option<&str>) -> Option<String> {
    match given {
        None => old.map(str::to_string),
        Some(value) => {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        }
    }
}

/// The live payment behind a cheque, if there is one. A voided payment is not
/// live: it is not counted in any balance, so it cannot be double-counted.
fn live_payment(conn: &Connection, payment_id: Option<i64>) -> Res<Option<i64>> {
    let Some(payment_id) = payment_id else {
        return Ok(None);
    };
    conn.query_row(
        "SELECT id FROM payment WHERE id = ?1 AND voided = 0",
        [payment_id],
        |r| r.get::<_, i64>(0),
    )
    .optional()
    .map_err(|_| "دفتر خوانده نشد".to_string())
}

/// A corrected cheque must not leave its payment telling a different story. When
/// the money itself changes, the payment is re-recorded and re-spread oldest
/// first over the party's open invoices — the same rule a typed-in payment
/// follows — so no invoice is left paid with money that has moved.
fn sync_payment(
    conn: &Connection,
    payment_id: i64,
    party_id: i64,
    direction: &str,
    amount: i64,
    reference: Option<&str>,
) -> Res<()> {
    let current = payments::load(conn, payment_id)?;
    if current.party_id != party_id {
        return Err("اول چک را از پاس دربیار، بعد طرف حسابش را عوض کن".to_string());
    }
    let reference = reference
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_string);
    if current.direction == direction && current.amount == amount && current.reference == reference
    {
        // Only a note or a bank name changed: the money stays where it landed.
        return Ok(());
    }
    payments::record(
        conn,
        &PaymentInput {
            id: Some(payment_id),
            party_id,
            direction: direction.to_string(),
            amount,
            day: current.day,
            method: "cheque".to_string(),
            reference,
            note: current.note.clone(),
            allocations: Vec::new(),
        },
        true,
    )?;
    Ok(())
}

/// Creates or corrects a cheque. A new cheque is always pending: only
/// [`set_status`] may turn it into money.
pub fn save(conn: &Connection, input: &ChequeInput) -> Res<Cheque> {
    let old = match input.id {
        Some(id) => Some(get(conn, id)?.cheque),
        None => None,
    };

    let party_id = input.party_id;
    if party_id <= 0 {
        return Err("طرف حساب را بزن".to_string());
    }
    let known: Option<i64> = conn
        .query_row("SELECT id FROM party WHERE id = ?1", [party_id], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|_| "دفتر خوانده نشد".to_string())?;
    if known.is_none() {
        return Err("طرف حساب پیدا نشد".to_string());
    }

    let direction = match input
        .direction
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        None => old
            .as_ref()
            .map(|c| c.direction.clone())
            .unwrap_or_else(|| "in".to_string()),
        Some("in") => "in".to_string(),
        Some("out") => "out".to_string(),
        Some(_) => return Err("چک را دریافتی یا دادنی بزن".to_string()),
    };

    let amount = input.amount;
    if amount <= 0 {
        return Err("مبلغ را بنویس".to_string());
    }
    let due_day = input.due_day;
    if due_day <= 0 {
        return Err("تاریخ سررسید را بنویس".to_string());
    }

    let number = text(
        input.number.as_deref(),
        old.as_ref().and_then(|c| c.number.as_deref()),
    );
    let bank = text(
        input.bank.as_deref(),
        old.as_ref().and_then(|c| c.bank.as_deref()),
    );
    let note = text(
        input.note.as_deref(),
        old.as_ref().and_then(|c| c.note.as_deref()),
    );

    let tx = conn
        .unchecked_transaction()
        .map_err(|_| "ذخیره نشد".to_string())?;

    let id = match &old {
        Some(current) => {
            if let Some(payment_id) = live_payment(&tx, current.payment_id)? {
                sync_payment(
                    &tx,
                    payment_id,
                    party_id,
                    &direction,
                    amount,
                    number.as_deref(),
                )?;
            }
            tx.execute(
                "UPDATE cheque SET party_id = ?1, direction = ?2, number = ?3, bank = ?4,
                        amount = ?5, due_day = ?6, note = ?7 WHERE id = ?8",
                params![party_id, direction, number, bank, amount, due_day, note, current.id],
            )
            .map_err(|_| "ذخیره نشد".to_string())?;
            db::audit(
                &tx,
                "cheque",
                current.id,
                "update",
                &json!({ "amount": amount, "dueDay": due_day, "direction": direction }).to_string(),
            )?;
            current.id
        }
        None => {
            tx.execute(
                "INSERT INTO cheque(party_id, direction, number, bank, amount, due_day, status,
                                    note, payment_id, created_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7, NULL, ?8)",
                params![
                    party_id,
                    direction,
                    number,
                    bank,
                    amount,
                    due_day,
                    note,
                    db::now_ms()
                ],
            )
            .map_err(|_| "ذخیره نشد".to_string())?;
            let id = tx.last_insert_rowid();
            db::audit(
                &tx,
                "cheque",
                id,
                "create",
                &json!({ "amount": amount, "dueDay": due_day, "direction": direction }).to_string(),
            )?;
            id
        }
    };

    tx.commit().map_err(|_| "ذخیره نشد".to_string())?;
    Ok(get(conn, id)?.cheque)
}

/// Moves a cheque to a new status and does the money work for it.
///
/// * `cleared` — records the payment the cheque stands for, spread over the
///   party's oldest open invoices of the matching kind the way a typed-in
///   payment is, and points `payment_id` at it. Whatever the invoices cannot
///   absorb stays as credit. A second `cleared` changes nothing.
/// * `bounced` / `returned` — voids that payment with a plain reason, and the
///   invoices it had paid go back to what they were. A bounced cheque forgets
///   the payment; a returned one keeps the record of it.
/// * `pending` — puts a settled cheque back on the stack, voiding its payment.
pub fn set_status(conn: &Connection, id: i64, status: &str, day: Option<i64>) -> Res<Cheque> {
    let status = status.trim();
    if !STATUSES.contains(&status) {
        return Err("مشکلی پیش آمد".to_string());
    }
    let before = get(conn, id)?.cheque;
    let live = live_payment(conn, before.payment_id)?;

    // The same status twice must not move money twice.
    if before.status == status && !(status == "cleared" && live.is_none()) {
        return Ok(before);
    }

    let day = day.filter(|d| *d > 0).unwrap_or_else(db::today);
    let tx = conn
        .unchecked_transaction()
        .map_err(|_| "وضعیت چک عوض نشد".to_string())?;

    match status {
        "cleared" => {
            let payment_id = match live {
                Some(payment_id) => payment_id,
                None => {
                    // The slip is money now: it pays down the oldest invoice of
                    // its kind first, and only what is left over is credit.
                    let payment = payments::record(
                        &tx,
                        &PaymentInput {
                            id: None,
                            party_id: before.party_id,
                            direction: before.direction.clone(),
                            amount: before.amount,
                            day,
                            method: "cheque".to_string(),
                            reference: before.number.clone(),
                            note: None,
                            allocations: Vec::new(),
                        },
                        true,
                    )?;
                    payment.id
                }
            };
            tx.execute(
                "UPDATE cheque SET status = 'cleared', payment_id = ?1 WHERE id = ?2",
                params![payment_id, id],
            )
            .map_err(|_| "وضعیت چک عوض نشد".to_string())?;
            db::audit(
                &tx,
                "cheque",
                id,
                "settle",
                &json!({ "paymentId": payment_id, "day": day }).to_string(),
            )?;
        }
        other => {
            let reason = match other {
                "bounced" => "چک برگشت خورد",
                "returned" => "چک پس داده شد",
                _ => "چک به انتظار برگشت",
            };
            if let Some(payment_id) = live {
                // The money is gone from the balance, so the invoices it had
                // paid must stop saying they were paid.
                let touched: Vec<i64> = {
                    let mut stmt = tx
                        .prepare("SELECT DISTINCT doc_id FROM allocation WHERE payment_id = ?1")
                        .map_err(|_| "وضعیت چک عوض نشد".to_string())?;
                    let found = stmt
                        .query_map([payment_id], |r| r.get(0))
                        .map_err(|_| "وضعیت چک عوض نشد".to_string())?
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_| "وضعیت چک عوض نشد".to_string())?;
                    found
                };
                tx.execute(
                    "UPDATE payment SET voided = 1, void_reason = ?1 WHERE id = ?2 AND voided = 0",
                    params![reason, payment_id],
                )
                .map_err(|_| "وضعیت چک عوض نشد".to_string())?;
                for doc_id in touched {
                    orders::refresh_status(&tx, doc_id)?;
                }
                db::audit(
                    &tx,
                    "payment",
                    payment_id,
                    "void",
                    &json!({ "reason": reason, "cheque": id }).to_string(),
                )?;
            }
            // A returned cheque keeps the pointer so the slip and its history
            // stay together; the rest forget it.
            let keep = if other == "returned" {
                before.payment_id
            } else {
                None
            };
            tx.execute(
                "UPDATE cheque SET status = ?1, payment_id = ?2 WHERE id = ?3",
                params![other, keep, id],
            )
            .map_err(|_| "وضعیت چک عوض نشد".to_string())?;
            db::audit(
                &tx,
                "cheque",
                id,
                "update",
                &json!({ "status": other, "reason": reason }).to_string(),
            )?;
        }
    }

    tx.commit().map_err(|_| "وضعیت چک عوض نشد".to_string())?;
    Ok(get(conn, id)?.cheque)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListArgs {
    status: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveArgs {
    cheque: ChequeInput,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusArgs {
    id: i64,
    status: String,
    day: Option<i64>,
}

pub fn handle(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    Some(match op {
        "cheques.list" => (|| {
            let a: ListArgs =
                serde_json::from_value(args.clone()).map_err(|_| "مشکلی پیش آمد".to_string())?;
            let conn = ctx.conn()?;
            Ok(serde_json::to_value(list(&conn, a.status.as_deref())?)
                .map_err(|_| "مشکلی پیش آمد".to_string())?)
        })(),
        "cheques.save" => (|| {
            let a: SaveArgs =
                serde_json::from_value(args.clone()).map_err(|_| "مشکلی پیش آمد".to_string())?;
            let conn = ctx.conn()?;
            Ok(serde_json::to_value(save(&conn, &a.cheque)?)
                .map_err(|_| "مشکلی پیش آمد".to_string())?)
        })(),
        "cheques.status" => (|| {
            let a: StatusArgs =
                serde_json::from_value(args.clone()).map_err(|_| "مشکلی پیش آمد".to_string())?;
            let conn = ctx.conn()?;
            Ok(
                serde_json::to_value(set_status(&conn, a.id, &a.status, a.day)?)
                    .map_err(|_| "مشکلی پیش آمد".to_string())?,
            )
        })(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::balance;

    fn seed(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO party(id, name, kind, opening, created_at, updated_at)
             VALUES(1, 'اکبر قصاب', 'customer', 0, 0, 0);
             INSERT INTO party(id, name, kind, opening, created_at, updated_at)
             VALUES(2, 'حاج‌آقا', 'supplier', 0, 0, 0);",
        )
        .unwrap();
    }

    /// A sale of 10,000 on credit: party 1 owes the shop that much.
    fn a_debt_of_ten_thousand(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO doc(id, kind, jyear, number, party_id, day, status, total, created_at, updated_at)
             VALUES(1, 'sale', 1404, 1, 1, 20000, 'open', 10000, 0, 0);",
        )
        .unwrap();
    }

    fn new_cheque(conn: &Connection, party_id: i64, amount: i64, due_day: i64) -> Cheque {
        save(
            conn,
            &ChequeInput {
                party_id,
                amount,
                due_day,
                number: Some("۱۲۳۴۵۶".to_string()),
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn payment_count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM payment", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn clearing_a_cheque_records_the_money_once() {
        let conn = db::memory();
        seed(&conn);
        a_debt_of_ten_thousand(&conn);
        assert_eq!(balance::of(&conn, 1).unwrap(), 10_000);

        let cheque = new_cheque(&conn, 1, 3_000, 20010);
        assert_eq!(cheque.status, "pending");
        assert_eq!(cheque.direction, "in");
        assert_eq!(balance::of(&conn, 1).unwrap(), 10_000);

        let cleared = set_status(&conn, cheque.id, "cleared", Some(20012)).unwrap();
        let payment_id = cleared.payment_id.expect("a cleared cheque has a payment");
        assert_eq!(cleared.status, "cleared");
        assert_eq!(payment_count(&conn), 1);
        assert_eq!(balance::of(&conn, 1).unwrap(), 7_000);

        // The invoice the slip was handed over for is paid down, not skipped.
        let doc = orders::load(&conn, 1).unwrap();
        assert_eq!((doc.paid, doc.remaining), (3_000, 7_000));

        // Saying «cleared» twice must not count the money twice.
        let again = set_status(&conn, cheque.id, "cleared", Some(20013)).unwrap();
        assert_eq!(again.payment_id, Some(payment_id));
        assert_eq!(payment_count(&conn), 1);
        assert_eq!(balance::of(&conn, 1).unwrap(), 7_000);

        // …and must not pay the invoice a second helping either.
        let doc = orders::load(&conn, 1).unwrap();
        assert_eq!((doc.paid, doc.remaining), (3_000, 7_000));
        let allocations: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM allocation WHERE payment_id = ?1",
                [payment_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(allocations, 1, "one payment, one share of one invoice");

        // The payment is the shop's own record of that cheque.
        let (method, reference, amount, day): (String, Option<String>, i64, i64) = conn
            .query_row(
                "SELECT method, ref, amount, day FROM payment WHERE id = ?1",
                [payment_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(method, "cheque");
        assert_eq!(reference.as_deref(), Some("۱۲۳۴۵۶"));
        assert_eq!(amount, 3_000);
        assert_eq!(day, 20012);
    }

    #[test]
    fn a_bounced_cheque_puts_the_balance_back() {
        let conn = db::memory();
        seed(&conn);
        a_debt_of_ten_thousand(&conn);

        let cheque = new_cheque(&conn, 1, 3_000, 20010);
        let cleared = set_status(&conn, cheque.id, "cleared", Some(20012)).unwrap();
        let payment_id = cleared.payment_id.unwrap();
        assert_eq!(balance::of(&conn, 1).unwrap(), 7_000);
        assert_eq!(orders::load(&conn, 1).unwrap().paid, 3_000);

        let bounced = set_status(&conn, cheque.id, "bounced", None).unwrap();
        assert_eq!(bounced.status, "bounced");
        assert_eq!(bounced.payment_id, None);
        assert_eq!(balance::of(&conn, 1).unwrap(), 10_000);
        assert_eq!(
            payment_count(&conn),
            1,
            "the payment is voided, not deleted"
        );

        // The invoice goes back to owing the whole of it.
        let doc = orders::load(&conn, 1).unwrap();
        assert_eq!((doc.paid, doc.remaining), (0, 10_000));
        assert_eq!(doc.status, "open");

        let (voided, reason): (i64, Option<String>) = conn
            .query_row(
                "SELECT voided, void_reason FROM payment WHERE id = ?1",
                [payment_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(voided, 1);
        assert!(reason.is_some_and(|r| !r.is_empty()));

        // And clearing it again after the bounce records the money afresh.
        let cleared_again = set_status(&conn, cheque.id, "cleared", Some(20020)).unwrap();
        assert_ne!(cleared_again.payment_id, Some(payment_id));
        assert_eq!(payment_count(&conn), 2);
        assert_eq!(balance::of(&conn, 1).unwrap(), 7_000);
    }

    #[test]
    fn money_from_a_party_who_owes_nothing_is_credit() {
        let conn = db::memory();
        seed(&conn);
        // No invoice at all: the party owes nothing before the cheque arrives.
        assert_eq!(balance::of(&conn, 2).unwrap(), 0);

        let cheque = new_cheque(&conn, 2, 5_000, 20010);
        set_status(&conn, cheque.id, "cleared", Some(20010)).unwrap();

        // Credit: money received that no invoice has used up yet.
        assert_eq!(balance::of(&conn, 2).unwrap(), -5_000);
        assert_eq!(payment_count(&conn), 1);
        let allocations: i64 = conn
            .query_row("SELECT COUNT(*) FROM allocation", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            allocations, 0,
            "with nothing to pay, the whole cheque is credit"
        );
    }

    #[test]
    fn a_cleared_cheque_pays_the_invoice_it_was_handed_over_for() {
        let conn = db::memory();
        seed(&conn);
        a_debt_of_ten_thousand(&conn);

        let cheque = new_cheque(&conn, 1, 10_000, 20010);
        set_status(&conn, cheque.id, "cleared", Some(20012)).unwrap();

        let doc = orders::load(&conn, 1).unwrap();
        assert_eq!(doc.paid, 10_000);
        assert_eq!(doc.remaining, 0);
        assert_eq!(doc.status, "settled", "a paid invoice stops being open");
        assert_eq!(balance::of(&conn, 1).unwrap(), 0);
    }

    #[test]
    fn the_oldest_invoice_is_paid_first_and_no_further() {
        let conn = db::memory();
        seed(&conn);
        conn.execute_batch(
            "INSERT INTO doc(id, kind, jyear, number, party_id, day, status, total, created_at, updated_at)
             VALUES(1, 'sale', 1404, 1, 1, 20000, 'open', 5000, 0, 0);
             INSERT INTO doc(id, kind, jyear, number, party_id, day, status, total, created_at, updated_at)
             VALUES(2, 'sale', 1404, 2, 1, 20001, 'open', 5000, 0, 0);",
        )
        .unwrap();

        let cheque = new_cheque(&conn, 1, 7_000, 20010);
        set_status(&conn, cheque.id, "cleared", Some(20012)).unwrap();

        let old = orders::load(&conn, 1).unwrap();
        let new = orders::load(&conn, 2).unwrap();
        assert_eq!((old.paid, old.status.as_str()), (5_000, "settled"));
        assert_eq!((new.paid, new.remaining), (2_000, 3_000));
        assert_eq!(balance::of(&conn, 1).unwrap(), 3_000);
    }

    #[test]
    fn a_cheque_i_pay_out_moves_the_other_way() {
        let conn = db::memory();
        seed(&conn);
        // The shop owes حاج‌آقا 40,000 for a purchase.
        conn.execute_batch(
            "INSERT INTO doc(id, kind, jyear, number, party_id, day, status, total, created_at, updated_at)
             VALUES(9, 'purchase', 1404, 1, 2, 20000, 'open', 40000, 0, 0);",
        )
        .unwrap();
        assert_eq!(balance::of(&conn, 2).unwrap(), -40_000);

        let cheque = save(
            &conn,
            &ChequeInput {
                party_id: 2,
                direction: Some("out".to_string()),
                amount: 40_000,
                due_day: 20005,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(cheque.direction, "out");
        assert_eq!(
            balance::of(&conn, 2).unwrap(),
            -40_000,
            "pending money is not money"
        );

        set_status(&conn, cheque.id, "cleared", Some(20005)).unwrap();
        assert_eq!(balance::of(&conn, 2).unwrap(), 0);
        // Money paid out settles the purchase, not a sale.
        assert_eq!(orders::load(&conn, 9).unwrap().status, "settled");
    }

    #[test]
    fn pending_cheques_come_first_and_soonest_first() {
        let conn = db::memory();
        seed(&conn);
        let late = new_cheque(&conn, 1, 1_000, 20030);
        let soon = new_cheque(&conn, 2, 2_000, 20010);
        let middle = new_cheque(&conn, 1, 3_000, 20020);
        set_status(&conn, late.id, "cleared", None).unwrap();

        let listed = list(&conn, None).unwrap();
        let ids: Vec<i64> = listed.iter().map(|r| r.cheque.id).collect();
        assert_eq!(ids, vec![soon.id, middle.id, late.id]);

        let only_pending = list(&conn, Some("pending")).unwrap();
        assert_eq!(only_pending.len(), 2);
        let only_cleared = list(&conn, Some("cleared")).unwrap();
        assert_eq!(only_cleared.len(), 1);
        assert_eq!(only_cleared[0].party_name, "اکبر قصاب");
    }

    #[test]
    fn a_cheque_needs_an_amount_and_a_date() {
        let conn = db::memory();
        seed(&conn);
        let no_amount = save(
            &conn,
            &ChequeInput {
                party_id: 1,
                amount: 0,
                due_day: 20010,
                ..Default::default()
            },
        );
        assert!(no_amount.is_err());
        let no_day = save(
            &conn,
            &ChequeInput {
                party_id: 1,
                amount: 1_000,
                ..Default::default()
            },
        );
        assert!(no_day.is_err());
        let no_party = save(
            &conn,
            &ChequeInput {
                amount: 1_000,
                due_day: 20010,
                ..Default::default()
            },
        );
        assert!(no_party.is_err());
    }

    #[test]
    fn correcting_a_cleared_cheque_corrects_its_payment() {
        let conn = db::memory();
        seed(&conn);
        a_debt_of_ten_thousand(&conn);
        let cheque = new_cheque(&conn, 1, 3_000, 20010);
        let cleared = set_status(&conn, cheque.id, "cleared", Some(20012)).unwrap();
        assert_eq!(balance::of(&conn, 1).unwrap(), 7_000);

        save(
            &conn,
            &ChequeInput {
                id: Some(cheque.id),
                party_id: 1,
                amount: 4_000,
                due_day: 20010,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(balance::of(&conn, 1).unwrap(), 6_000);
        assert_eq!(
            orders::load(&conn, 1).unwrap().remaining,
            6_000,
            "the invoice follows the corrected amount"
        );

        let payment_amount: i64 = conn
            .query_row(
                "SELECT amount FROM payment WHERE id = ?1",
                [cleared.payment_id.unwrap()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(payment_amount, 4_000);

        let allocated: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(amount), 0) FROM allocation WHERE payment_id = ?1",
                [cleared.payment_id.unwrap()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(allocated, 4_000, "the share moved with the money");
    }
}
