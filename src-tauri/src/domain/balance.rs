//! Party balances.
//!
//! One formula, used by every screen that shows a number:
//!
//! ```text
//! balance = opening
//!         + (sales total      − sales paid back)
//!         − (purchases total  − purchases paid back)
//!         − (money received and not yet applied to an invoice)
//!         + (money paid out and not yet applied to a purchase)
//! ```
//!
//! Positive means the party owes the shop. Negative means the shop owes the
//! party. The interface never shows a bare sign: it says «طلب» or «بدهی».

use crate::db::Res;
use crate::dto::Totals;
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;

/// Sales still unpaid, purchases still unpaid, and loose credit, per party.
const ALL_BALANCES: &str = r#"
SELECT p.id,
       p.opening
     + COALESCE(s.rem, 0) - COALESCE(pu.rem, 0)
     - COALESCE(pin.loose, 0) + COALESCE(pout.loose, 0) AS balance
FROM party p
LEFT JOIN (
    SELECT d.party_id,
           SUM(d.total - COALESCE((SELECT SUM(a.amount) FROM allocation a
                                   JOIN payment pm ON pm.id = a.payment_id
                                   WHERE a.doc_id = d.id AND pm.voided = 0), 0)) AS rem
    FROM doc d
    WHERE d.kind = 'sale' AND d.status <> 'void'
    GROUP BY d.party_id
) s ON s.party_id = p.id
LEFT JOIN (
    SELECT d.party_id,
           SUM(d.total - COALESCE((SELECT SUM(a.amount) FROM allocation a
                                   JOIN payment pm ON pm.id = a.payment_id
                                   WHERE a.doc_id = d.id AND pm.voided = 0), 0)) AS rem
    FROM doc d
    WHERE d.kind = 'purchase' AND d.status <> 'void'
    GROUP BY d.party_id
) pu ON pu.party_id = p.id
LEFT JOIN (
    SELECT pm.party_id,
           SUM(pm.amount - COALESCE((SELECT SUM(a.amount) FROM allocation a
                                     WHERE a.payment_id = pm.id), 0)) AS loose
    FROM payment pm
    WHERE pm.direction = 'in' AND pm.voided = 0
    GROUP BY pm.party_id
) pin ON pin.party_id = p.id
LEFT JOIN (
    SELECT pm.party_id,
           SUM(pm.amount - COALESCE((SELECT SUM(a.amount) FROM allocation a
                                     WHERE a.payment_id = pm.id), 0)) AS loose
    FROM payment pm
    WHERE pm.direction = 'out' AND pm.voided = 0
    GROUP BY pm.party_id
) pout ON pout.party_id = p.id
"#;

pub fn all(conn: &Connection) -> Res<HashMap<i64, i64>> {
    let mut stmt = conn
        .prepare(ALL_BALANCES)
        .map_err(|e| crate::db::oops("محاسبه حساب‌ها نشد", e))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
        .map_err(|e| crate::db::oops("محاسبه حساب‌ها نشد", e))?;
    let mut out = HashMap::new();
    for row in rows {
        let (id, balance) = row.map_err(|e| crate::db::oops("محاسبه حساب‌ها نشد", e))?;
        out.insert(id, balance);
    }
    Ok(out)
}

pub fn of(conn: &Connection, party_id: i64) -> Res<i64> {
    conn.query_row(
        &format!("SELECT balance FROM ({ALL_BALANCES}) WHERE id = ?1"),
        [party_id],
        |r| r.get::<_, i64>(0),
    )
    .optional()
    .map_err(|e| crate::db::oops("محاسبه حساب نشد", e))
    .map(|v| v.unwrap_or(0))
}

/// The two numbers on the home banner.
pub fn totals(conn: &Connection) -> Res<Totals> {
    let rows = all(conn)?;
    let mut owes_me = 0i64;
    let mut i_owe = 0i64;
    for balance in rows.values() {
        if *balance > 0 {
            owes_me += balance;
        } else {
            i_owe += -balance;
        }
    }
    Ok(Totals { owes_me, i_owe })
}

/// How much of a document has actually been paid.
pub fn paid_on(conn: &Connection, doc_id: i64) -> Res<i64> {
    conn.query_row(
        "SELECT COALESCE(SUM(a.amount), 0) FROM allocation a
         JOIN payment p ON p.id = a.payment_id
         WHERE a.doc_id = ?1 AND p.voided = 0",
        [doc_id],
        |r| r.get::<_, i64>(0),
    )
    .map_err(|e| crate::db::oops("محاسبه پرداخت نشد", e))
}

pub fn unallocated(conn: &Connection, payment_id: i64) -> Res<i64> {
    conn.query_row(
        "SELECT p.amount - COALESCE((SELECT SUM(a.amount) FROM allocation a WHERE a.payment_id = p.id), 0)
         FROM payment p WHERE p.id = ?1",
        [payment_id],
        |r| r.get::<_, i64>(0),
    )
    .map_err(|e| crate::db::oops("محاسبه مانده نشد", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn seed(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO party(id, name, kind, opening, created_at, updated_at)
             VALUES(1, 'لیلا امینی', 'customer', 0, 0, 0);
             INSERT INTO doc(id, kind, jyear, number, party_id, day, status, total, created_at, updated_at)
             VALUES(1, 'sale', 1404, 1, 1, 20000, 'open', 5175, 0, 0);
             INSERT INTO doc(id, kind, jyear, number, party_id, day, status, total, created_at, updated_at)
             VALUES(2, 'sale', 1404, 2, 1, 20001, 'open', 5700, 0, 0);",
        )
        .unwrap();
    }

    #[test]
    fn unpaid_invoices_add_up() {
        let conn = db::memory();
        seed(&conn);
        assert_eq!(of(&conn, 1).unwrap(), 10_875);
    }

    #[test]
    fn a_partial_payment_leaves_the_remainder() {
        let conn = db::memory();
        seed(&conn);
        // 3,000 handed over and applied to the first invoice.
        conn.execute_batch(
            "INSERT INTO payment(id, party_id, direction, amount, day, method, created_at)
             VALUES(1, 1, 'in', 3000, 20002, 'cash', 0);
             INSERT INTO allocation(payment_id, doc_id, amount) VALUES(1, 1, 3000);",
        )
        .unwrap();
        assert_eq!(of(&conn, 1).unwrap(), 7_875);
        assert_eq!(paid_on(&conn, 1).unwrap(), 3_000);
    }

    #[test]
    fn unapplied_money_is_credit_not_a_discount() {
        let conn = db::memory();
        seed(&conn);
        conn.execute_batch(
            "INSERT INTO payment(id, party_id, direction, amount, day, method, created_at)
             VALUES(1, 1, 'in', 2000, 20002, 'cash', 0);",
        )
        .unwrap();
        assert_eq!(unallocated(&conn, 1).unwrap(), 2_000);
        assert_eq!(of(&conn, 1).unwrap(), 8_875);
    }

    #[test]
    fn a_voided_payment_stops_counting() {
        let conn = db::memory();
        seed(&conn);
        conn.execute_batch(
            "INSERT INTO payment(id, party_id, direction, amount, day, method, created_at, voided)
             VALUES(1, 1, 'in', 3000, 20002, 'cash', 0, 1);
             INSERT INTO allocation(payment_id, doc_id, amount) VALUES(1, 1, 3000);",
        )
        .unwrap();
        assert_eq!(of(&conn, 1).unwrap(), 10_875);
    }

    #[test]
    fn purchases_make_me_the_debtor() {
        let conn = db::memory();
        conn.execute_batch(
            "INSERT INTO party(id, name, kind, opening, created_at, updated_at)
             VALUES(2, 'حاج‌آقا', 'supplier', 0, 0, 0);
             INSERT INTO doc(id, kind, jyear, number, party_id, day, status, total, created_at, updated_at)
             VALUES(9, 'purchase', 1404, 1, 2, 20000, 'open', 40000, 0, 0);",
        )
        .unwrap();
        assert_eq!(of(&conn, 2).unwrap(), -40_000);
        let t = totals(&conn).unwrap();
        assert_eq!(t.i_owe, 40_000);
        assert_eq!(t.owes_me, 0);
    }
}
