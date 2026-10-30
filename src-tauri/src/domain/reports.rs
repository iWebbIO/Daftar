//! Totals, debt ageing and per-product figures.
//!
//! Everything here answers one of the shopkeeper's questions: «چقدر فروختم»
//! (how much did I sell), «چه چیزی بیشتر فروش داشته» (what sells best) and
//! «چقدر پول دست مردم مانده» (how much money is still out there).

use crate::db::{self, Res};
use crate::domain::products::{fail, normalise};
use crate::dto::{AgeBucket, ProductReport, ReportDays};
use crate::Ctx;
use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

/// Money that is young, money that is getting old, money that is too old.
const BUCKETS: [&str; 3] = ["تا ۷ روز", "۸ تا ۳۰ روز", "بیش از ۳۰ روز"];
/// The product report is a shortlist, not a ledger.
const PRODUCTS_MAX: usize = 60;

/// One row per day that saw any activity, oldest first.
pub fn days(conn: &Connection, from: i64, to: i64) -> Res<Vec<ReportDays>> {
    if from > to {
        return Ok(Vec::new());
    }
    let mut per_day: BTreeMap<i64, ReportDays> = BTreeMap::new();

    {
        let mut stmt = conn
            .prepare(
                "SELECT d.day, d.kind, COALESCE(SUM(d.total), 0), COUNT(*)
                 FROM doc d
                 WHERE d.status <> 'void' AND d.day BETWEEN ?1 AND ?2
                 GROUP BY d.day, d.kind",
            )
            .map_err(|e| fail("گزارش روزها خوانده نشد", e))?;
        let rows = stmt
            .query_map([from, to], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| fail("گزارش روزها خوانده نشد", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| fail("گزارش روزها خوانده نشد", e))?;
        for (day, kind, total, count) in rows {
            let entry = per_day.entry(day).or_insert(ReportDays {
                day,
                sold: 0,
                bought: 0,
                received: 0,
                paid: 0,
                count: 0,
            });
            if kind == "sale" {
                entry.sold += total;
            } else {
                entry.bought += total;
            }
            entry.count += count;
        }
    }

    {
        let mut stmt = conn
            .prepare(
                "SELECT day, direction, COALESCE(SUM(amount), 0)
                 FROM payment
                 WHERE voided = 0 AND day BETWEEN ?1 AND ?2
                 GROUP BY day, direction",
            )
            .map_err(|e| fail("گزارش روزها خوانده نشد", e))?;
        let rows = stmt
            .query_map([from, to], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| fail("گزارش روزها خوانده نشد", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| fail("گزارش روزها خوانده نشد", e))?;
        for (day, direction, amount) in rows {
            let entry = per_day.entry(day).or_insert(ReportDays {
                day,
                sold: 0,
                bought: 0,
                received: 0,
                paid: 0,
                count: 0,
            });
            if direction == "in" {
                entry.received += amount;
            } else {
                entry.paid += amount;
            }
        }
    }

    Ok(per_day.into_values().collect())
}

struct Tally {
    product_id: Option<i64>,
    name: String,
    unit: String,
    qty_milli: i64,
    revenue: i64,
    /// The price on the newest line of the range, used when nothing has been
    /// recorded in `price_history`.
    newest_price: i64,
}

/// What sold, how much of it, and for how much. Sales only: a purchase is not
/// income, it is the cost of one.
pub fn products(conn: &Connection, from: i64, to: i64) -> Res<Vec<ProductReport>> {
    if from > to {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare(
            "SELECT l.product_id, l.name, l.unit, l.qty_milli,
                    COALESCE(l.override_total, l.total), l.price
             FROM doc_line l JOIN doc d ON d.id = l.doc_id
             WHERE d.kind = 'sale' AND d.status <> 'void' AND l.voided = 0
               AND d.day BETWEEN ?1 AND ?2
             ORDER BY d.day DESC, d.id DESC, l.pos DESC",
        )
        .map_err(|e| fail("گزارش کالاها خوانده نشد", e))?;
    let rows = stmt
        .query_map([from, to], |r| {
            Ok((
                r.get::<_, Option<i64>>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })
        .map_err(|e| fail("گزارش کالاها خوانده نشد", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| fail("گزارش کالاها خوانده نشد", e))?;

    // A kilo and a piece never go in the same row, so the unit is part of the key.
    // Rows come newest first, so the first spelling seen is the one shown.
    let mut order: Vec<String> = Vec::new();
    let mut tally: HashMap<String, Tally> = HashMap::new();
    for (product_id, name, unit, qty, revenue, price) in rows {
        let key = match product_id {
            Some(id) => format!("p{id}\u{1F}{unit}"),
            None => format!("n{}\u{1F}{unit}", normalise(&name)),
        };
        match tally.get_mut(&key) {
            Some(t) => {
                t.qty_milli += qty;
                t.revenue += revenue;
            }
            None => {
                order.push(key.clone());
                tally.insert(
                    key,
                    Tally {
                        product_id,
                        name,
                        unit,
                        qty_milli: qty,
                        revenue,
                        newest_price: price,
                    },
                );
            }
        }
    }

    let mut out = Vec::with_capacity(order.len());
    for key in order {
        let Some(t) = tally.remove(&key) else { continue };
        let last_price = match t.product_id {
            Some(id) => latest_sale_price(conn, id)?.unwrap_or(t.newest_price),
            None => t.newest_price,
        };
        out.push(ProductReport {
            product_id: t.product_id,
            name: t.name,
            qty_milli: t.qty_milli,
            unit: t.unit,
            revenue: t.revenue,
            last_price,
        });
    }

    out.sort_by(|a, b| b.revenue.cmp(&a.revenue).then_with(|| a.name.cmp(&b.name)));
    out.truncate(PRODUCTS_MAX);
    Ok(out)
}

fn latest_sale_price(conn: &Connection, product_id: i64) -> Res<Option<i64>> {
    conn.query_row(
        "SELECT price FROM price_history
         WHERE product_id = ?1 AND kind = 'sale'
         ORDER BY at DESC, id DESC LIMIT 1",
        [product_id],
        |r| r.get::<_, i64>(0),
    )
    .optional()
    .map_err(|e| fail("آخرین قیمت خوانده نشد", e))
}

/// How long the money that is still out there has been waiting.
///
/// Each invoice is aged on its own, so the amount in a bucket is what is really
/// left on it — not what it was worth the day it was written.
pub fn ageing(conn: &Connection) -> Res<Vec<AgeBucket>> {
    let today = db::today();
    let mut buckets = [
        AgeBucket {
            label: BUCKETS[0].to_string(),
            amount: 0,
            count: 0,
        },
        AgeBucket {
            label: BUCKETS[1].to_string(),
            amount: 0,
            count: 0,
        },
        AgeBucket {
            label: BUCKETS[2].to_string(),
            amount: 0,
            count: 0,
        },
    ];

    let mut stmt = conn
        .prepare(
            "SELECT d.day,
                    d.total - COALESCE((SELECT SUM(a.amount) FROM allocation a
                                        JOIN payment pm ON pm.id = a.payment_id
                                        WHERE a.doc_id = d.id AND pm.voided = 0), 0)
             FROM doc d
             WHERE d.kind = 'sale' AND d.status <> 'void'",
        )
        .map_err(|e| fail("گزارش بدهی‌ها خوانده نشد", e))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
        .map_err(|e| fail("گزارش بدهی‌ها خوانده نشد", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| fail("گزارش بدهی‌ها خوانده نشد", e))?;

    for (day, remaining) in rows {
        if remaining <= 0 {
            continue; // paid in full, or overpaid: no longer a debt
        }
        let age = today - day;
        let bucket = if age <= 7 {
            0
        } else if age <= 30 {
            1
        } else {
            2
        };
        buckets[bucket].amount += remaining;
        buckets[bucket].count += 1;
    }

    Ok(buckets.to_vec())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RangeArgs {
    from: Option<i64>,
    to: Option<i64>,
}

/// A report always covers a real span: no dates means today, which is what a
/// screen that forgot to send them deserves.
fn range(a: &RangeArgs) -> (i64, i64) {
    let to = a.to.unwrap_or_else(db::today);
    (a.from.unwrap_or(to), to)
}

pub fn handle(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    Some(match op {
        "reports.days" => (|| -> Res<Value> {
            let a: RangeArgs = serde_json::from_value(args.clone()).unwrap_or_default();
            let (from, to) = range(&a);
            let conn = ctx.conn()?;
            let rows = days(&conn, from, to)?;
            serde_json::to_value(rows).map_err(|e| fail("گزارش روزها ساخته نشد", e))
        })(),
        "reports.products" => (|| -> Res<Value> {
            let a: RangeArgs = serde_json::from_value(args.clone()).unwrap_or_default();
            let (from, to) = range(&a);
            let conn = ctx.conn()?;
            let rows = products(&conn, from, to)?;
            serde_json::to_value(rows).map_err(|e| fail("گزارش کالاها ساخته نشد", e))
        })(),
        "reports.ageing" => (|| -> Res<Value> {
            let conn = ctx.conn()?;
            let rows = ageing(&conn)?;
            serde_json::to_value(rows).map_err(|e| fail("گزارش بدهی‌ها ساخته نشد", e))
        })(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use rusqlite::Connection;

    const NOK: &str = "\u{646}\u{62E}\u{648}\u{62F}";
    const NOK_ZWNJ: &str = "\u{646}\u{62E}\u{200C}\u{648}\u{62F}";
    const KG: &str = "\u{6A9}\u{6CC}\u{644}\u{648}";

    fn add_party(conn: &Connection, id: i64) {
        conn.execute(
            "INSERT INTO party(id, name, kind, created_at, updated_at)
             VALUES(?1, 'لیلا امینی', 'customer', 0, 0)",
            [id],
        )
        .unwrap();
    }

    fn add_product(conn: &Connection, id: i64, name: &str, unit: &str) {
        conn.execute(
            "INSERT INTO product(id, name, unit, active, created_at, updated_at)
             VALUES(?1, ?2, ?3, 1, 0, 0)",
            rusqlite::params![id, name, unit],
        )
        .unwrap();
    }

    fn add_doc(conn: &Connection, id: i64, kind: &str, day: i64, status: &str, total: i64) {
        conn.execute(
            "INSERT INTO doc(id, kind, jyear, number, party_id, day, status, total, created_at, updated_at)
             VALUES(?1, ?2, 1404, ?1, 1, ?3, ?4, ?5, 0, 0)",
            rusqlite::params![id, kind, day, status, total],
        )
        .unwrap();
    }

    #[allow(clippy::too_many_arguments)]
    fn add_line(
        conn: &Connection,
        id: i64,
        doc: i64,
        pos: i64,
        product: Option<i64>,
        name: &str,
        qty_milli: i64,
        price: i64,
        total: i64,
        voided: bool,
    ) {
        conn.execute(
            "INSERT INTO doc_line(id, doc_id, pos, product_id, name, qty_milli, unit, price, total, voided)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![id, doc, pos, product, name, qty_milli, KG, price, total, voided as i64],
        )
        .unwrap();
    }

    fn add_payment(conn: &Connection, id: i64, direction: &str, amount: i64, day: i64, voided: bool) {
        conn.execute(
            "INSERT INTO payment(id, party_id, direction, amount, day, method, voided, created_at)
             VALUES(?1, 1, ?2, ?3, ?4, 'cash', ?5, 0)",
            rusqlite::params![id, direction, amount, day, voided as i64],
        )
        .unwrap();
    }

    fn add_allocation(conn: &Connection, payment: i64, doc: i64, amount: i64) {
        conn.execute(
            "INSERT INTO allocation(payment_id, doc_id, amount) VALUES(?1, ?2, ?3)",
            rusqlite::params![payment, doc, amount],
        )
        .unwrap();
    }

    /// Test 4: two invoices of the same goods add up; crossed-out things do not.
    #[test]
    fn report_products_adds_up_two_invoices() {
        let conn = db::memory();
        add_party(&conn, 1);
        add_product(&conn, 5, NOK, KG);

        // 15 kg at 345, then 5 kg at 400.
        add_doc(&conn, 1, "sale", 20_000, "open", 5_175);
        add_line(&conn, 1, 1, 1, Some(5), NOK, 15_000, 345, 5_175, false);
        add_doc(&conn, 2, "sale", 20_001, "open", 2_000);
        add_line(&conn, 2, 2, 1, Some(5), NOK, 5_000, 400, 2_000, false);
        // A line he crossed out inside a good invoice.
        add_line(&conn, 3, 2, 2, Some(5), NOK, 1_000, 400, 400, true);
        // A cancelled invoice.
        add_doc(&conn, 3, "sale", 20_002, "void", 10_000);
        add_line(&conn, 4, 3, 1, Some(5), NOK, 99_000, 400, 10_000, false);
        // A purchase is stock, not income.
        add_doc(&conn, 4, "purchase", 20_000, "open", 3_000);
        add_line(&conn, 5, 4, 1, Some(5), NOK, 2_000, 400, 3_000, false);
        // The last price he actually charged.
        conn.execute_batch(
            "INSERT INTO price_history(product_id, party_id, kind, price, day, at)
             VALUES(5, NULL, 'sale', 345, 20000, 1000);
             INSERT INTO price_history(product_id, party_id, kind, price, day, at)
             VALUES(5, NULL, 'sale', 400, 20001, 2000);",
        )
        .unwrap();

        let rows = products(&conn, 20_000, 20_002).unwrap();
        assert_eq!(rows.len(), 1, "one row per product");
        let row = &rows[0];
        assert_eq!(row.product_id, Some(5));
        assert_eq!(row.name, NOK);
        assert_eq!(row.unit, KG);
        assert_eq!(row.qty_milli, 20_000, "15 kg + 5 kg, the crossed-out kilo ignored");
        assert_eq!(row.revenue, 7_175, "5,175 + 2,000");
        assert_eq!(row.last_price, 400);

        // The window decides what is in the report.
        let narrow = products(&conn, 20_001, 20_001).unwrap();
        assert_eq!(narrow.len(), 1);
        assert_eq!(narrow[0].qty_milli, 5_000);
        assert_eq!(narrow[0].revenue, 2_000);
    }

    #[test]
    fn report_products_keeps_untyped_items_apart_from_real_ones() {
        let conn = db::memory();
        add_party(&conn, 1);
        add_product(&conn, 5, NOK, KG);

        add_doc(&conn, 1, "sale", 20_000, "open", 5_175);
        add_line(&conn, 1, 1, 1, Some(5), NOK, 15_000, 345, 5_175, false);
        // Two lines he typed by hand, spelled slightly differently: one row.
        add_doc(&conn, 2, "sale", 20_001, "open", 600);
        add_line(&conn, 2, 2, 1, None, NOK, 1_000, 600, 600, false);
        add_line(&conn, 3, 2, 2, None, NOK_ZWNJ, 1_000, 600, 600, false);

        let rows = products(&conn, 20_000, 20_001).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].product_id, Some(5));
        assert_eq!(rows[0].revenue, 5_175, "biggest money first");
        assert_eq!(rows[1].product_id, None);
        assert_eq!(rows[1].qty_milli, 2_000);
        assert_eq!(rows[1].revenue, 1_200);
        assert_eq!(normalise(&rows[1].name), normalise(NOK));
        assert_eq!(rows[1].last_price, 600);
    }

    #[test]
    fn report_days_skips_the_quiet_days() {
        let conn = db::memory();
        add_party(&conn, 1);
        add_doc(&conn, 1, "sale", 20_000, "open", 1_000);
        add_doc(&conn, 2, "purchase", 20_000, "open", 500);
        add_doc(&conn, 3, "sale", 20_002, "open", 700);
        add_doc(&conn, 4, "sale", 20_001, "void", 9_000);
        add_payment(&conn, 1, "in", 300, 20_000, false);
        add_payment(&conn, 2, "out", 200, 20_003, false);
        add_payment(&conn, 3, "in", 8_000, 20_003, true); // cancelled, not money

        let rows = days(&conn, 19_999, 20_004).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.day).collect::<Vec<_>>(),
            vec![20_000, 20_002, 20_003],
            "one row per day that saw something, oldest first"
        );
        assert_eq!(rows[0].sold, 1_000);
        assert_eq!(rows[0].bought, 500);
        assert_eq!(rows[0].received, 300);
        assert_eq!(rows[0].count, 2, "two documents");
        assert_eq!(rows[1].sold, 700);
        assert_eq!(rows[1].count, 1);
        assert_eq!(rows[2].paid, 200);
        assert_eq!(rows[2].count, 0, "money moved, no invoice");

        assert!(days(&conn, 40_000, 40_001).unwrap().is_empty());
        assert!(days(&conn, 20_004, 20_000).unwrap().is_empty(), "backwards span");
    }

    /// Test 5: 40 days old is past thirty, 3 days old is still young, and the
    /// amount is what is *left*, not what the invoice said.
    #[test]
    fn ageing_splits_the_money_by_how_long_it_has_waited() {
        let conn = db::memory();
        add_party(&conn, 1);
        let today = db::today();

        // 40 days old, 4,000 of 10,000 paid: 6,000 is the debt.
        add_doc(&conn, 1, "sale", today - 40, "open", 10_000);
        add_payment(&conn, 1, "in", 4_000, today - 39, false);
        add_allocation(&conn, 1, 1, 4_000);
        // 3 days old and untouched: the whole 8,000 is out there.
        add_doc(&conn, 2, "sale", today - 3, "open", 8_000);
        // 15 days old, 1,000 paid: 4,000 left.
        add_doc(&conn, 3, "sale", today - 15, "open", 5_000);
        add_payment(&conn, 2, "in", 1_000, today - 14, false);
        add_allocation(&conn, 2, 3, 1_000);
        // Paid in full: nothing outstanding, so nothing to age.
        add_doc(&conn, 4, "sale", today - 2, "settled", 3_000);
        add_payment(&conn, 3, "in", 3_000, today - 2, false);
        add_allocation(&conn, 3, 4, 3_000);
        // A cancelled invoice is not a debt.
        add_doc(&conn, 5, "sale", today - 1, "void", 9_000);
        // Paid with a cancelled receipt: the money never arrived, so it counts.
        add_doc(&conn, 6, "sale", today - 1, "open", 2_000);
        add_payment(&conn, 4, "in", 2_000, today - 1, true);
        add_allocation(&conn, 4, 6, 2_000);

        let buckets = ageing(&conn).unwrap();
        let labels: Vec<&str> = buckets.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, vec!["تا ۷ روز", "۸ تا ۳۰ روز", "بیش از ۳۰ روز"]);

        assert_eq!(buckets[0].amount, 10_000, "8,000 + the 2,000 whose receipt was cancelled");
        assert_eq!(buckets[0].count, 2);
        assert_eq!(buckets[1].amount, 4_000, "the remaining, not the 5,000 total");
        assert_eq!(buckets[1].count, 1);
        assert_eq!(buckets[2].amount, 6_000, "10,000 − 4,000 paid");
        assert_eq!(buckets[2].count, 1);
    }

    #[test]
    fn handle_answers_only_its_own_names() {
        let state = crate::AppState {
            db: std::sync::Mutex::new(db::memory()),
            dirs: crate::Dirs {
                data: std::path::PathBuf::new(),
                files: std::path::PathBuf::new(),
            },
        };
        let ctx = Ctx { state: &state };
        assert!(handle(&ctx, "reports.ageing", &serde_json::json!({})).is_some());
        assert!(handle(&ctx, "reports.days", &serde_json::json!({ "from": 1, "to": 2 })).is_some());
        assert!(handle(&ctx, "search.all", &serde_json::json!({ "q": "x" })).is_none());

        let buckets = handle(&ctx, "reports.ageing", &serde_json::json!({}))
            .unwrap()
            .unwrap();
        let buckets: Vec<AgeBucket> = serde_json::from_value(buckets).unwrap();
        assert_eq!(buckets.len(), 3);
        assert_eq!(buckets[0].amount, 0);
    }
}
