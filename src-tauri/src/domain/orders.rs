//! Invoices: the order screen, and everything saving one writes.

use crate::db::Res;
use crate::dto::{Doc, DocRow};
use crate::Ctx;
use rusqlite::Connection;
use serde_json::Value;

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
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([day], |r| {
            Ok(DocRow {
                doc: from_row(r)?,
                party_name: r.get(16)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

pub fn handle(_ctx: &Ctx<'_>, _op: &str, _args: &Value) -> Option<Res<Value>> {
    None
}
