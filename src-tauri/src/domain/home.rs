//! The Today screen: one day of the shop, the numbers on its banner, and the
//! reminders that belong to it.

use crate::db::{self, Res};
use crate::domain::{balance, orders, settings};
use crate::dto::{ChequeRow, DocRow, TaskRow, TodayView};
use crate::jalali;
use crate::Ctx;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DayArgs {
    day: Option<i64>,
}

pub fn build(conn: &Connection, day: i64) -> Res<TodayView> {
    let settings = settings::load(conn);

    let mut sold = 0i64;
    let mut bought = 0i64;
    let mut received = 0i64;
    let mut paid = 0i64;

    {
        let mut stmt = conn
            .prepare(
                "SELECT d.kind, d.total FROM doc d
                 WHERE d.day = ?1 AND d.status <> 'void'",
            )
            .map_err(|e| db::oops("دفتر خوانده نشد", e))?;
        let rows = stmt
            .query_map([day], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| db::oops("دفتر خوانده نشد", e))?;
        for row in rows {
            let (kind, total) = row.map_err(|e| db::oops("دفتر خوانده نشد", e))?;
            if kind == "sale" {
                sold += total;
            } else {
                bought += total;
            }
        }
    }

    {
        let mut stmt = conn
            .prepare(
                "SELECT direction, amount FROM payment
                 WHERE day = ?1 AND voided = 0",
            )
            .map_err(|e| db::oops("دفتر خوانده نشد", e))?;
        let rows = stmt
            .query_map([day], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| db::oops("دفتر خوانده نشد", e))?;
        for row in rows {
            let (direction, amount) = row.map_err(|e| db::oops("دفتر خوانده نشد", e))?;
            if direction == "in" {
                received += amount;
            } else {
                paid += amount;
            }
        }
    }

    let docs = orders::rows_on_day(conn, day)?;

    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.text, t.day, t.party_id, t.done, t.created_at, p.name
             FROM task t LEFT JOIN party p ON p.id = t.party_id
             WHERE t.done = 0 AND (t.day IS NULL OR t.day <= ?1)
             ORDER BY COALESCE(t.day, 0) DESC, t.id DESC LIMIT 40",
        )
        .map_err(|e| db::oops("دفتر خوانده نشد", e))?;
    let tasks = stmt
        .query_map([day], |r| {
            Ok(TaskRow {
                task: crate::dto::Task {
                    id: r.get(0)?,
                    text: r.get(1)?,
                    day: r.get(2)?,
                    party_id: r.get(3)?,
                    done: r.get::<_, i64>(4)? != 0,
                    created_at: r.get(5)?,
                },
                party_name: r.get(6)?,
            })
        })
        .map_err(|e| db::oops("دفتر خوانده نشد", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| db::oops("دفتر خوانده نشد", e))?;

    let mut stmt = conn
        .prepare(
            "SELECT c.id, c.party_id, c.direction, c.number, c.bank, c.amount, c.due_day,
                    c.status, c.note, c.payment_id, c.created_at, p.name
             FROM cheque c JOIN party p ON p.id = c.party_id
             WHERE c.status = 'pending' AND c.due_day <= ?1 + 7
             ORDER BY c.due_day ASC LIMIT 20",
        )
        .map_err(|e| db::oops("دفتر خوانده نشد", e))?;
    let cheques = stmt
        .query_map([day], |r| {
            Ok(ChequeRow {
                cheque: crate::dto::Cheque {
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
        })
        .map_err(|e| db::oops("دفتر خوانده نشد", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| db::oops("دفتر خوانده نشد", e))?;

    let totals = balance::totals(conn)?;
    let j = jalali::d2j(day);

    Ok(TodayView {
        day,
        jalali: j.short(),
        jalali_long: j.long(),
        weekday: jalali::weekday_name(day).to_string(),
        sold,
        bought,
        received,
        paid,
        docs,
        tasks,
        cheques,
        totals,
        last_backup_at: settings.last_backup_at,
    })
}

pub fn handle(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    Some(match op {
        "home.today" => (|| {
            let a: DayArgs = serde_json::from_value(args.clone()).unwrap_or(DayArgs { day: None });
            let day = a.day.unwrap_or_else(db::today);
            let conn = ctx.conn()?;
            Ok(serde_json::to_value(build(&conn, day)?).map_err(|e| db::oops("دفتر خوانده نشد", e))?)
        })(),
        _ => return None,
    })
}

#[allow(dead_code)]
fn unused_doc_row(_: DocRow) {}
