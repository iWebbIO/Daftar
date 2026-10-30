//! Sum sheets: the numbered columns of the paper notebook.
//!
//! A sheet is one page of arithmetic: a column of labelled amounts, some of them
//! struck out, and a total that counts only the ones still standing. The sheet
//! is the unit of editing, so its rows are written in one go.

use crate::db::{self, Res};
use crate::dto::{SumRow, SumSheet, SumSheetInput};
use crate::Ctx;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

fn rows_of(conn: &Connection, sheet_id: i64) -> Res<Vec<SumRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, sheet_id, pos, label, amount, party_id, struck
             FROM sum_row WHERE sheet_id = ?1 ORDER BY pos, id",
        )
        .map_err(|_| "دفتر خوانده نشد".to_string())?;
    let rows = stmt
        .query_map([sheet_id], |r| {
            Ok(SumRow {
                id: r.get(0)?,
                sheet_id: r.get(1)?,
                pos: r.get(2)?,
                label: r.get(3)?,
                amount: r.get(4)?,
                party_id: r.get(5)?,
                struck: r.get(6)?,
            })
        })
        .map_err(|_| "دفتر خوانده نشد".to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "دفتر خوانده نشد".to_string())?;
    Ok(rows)
}

/// One sheet with its rows and its total. The total is worked out on every
/// read, so it cannot go stale: struck rows are the shopkeeper's crossed-out
/// lines and do not count.
pub fn get(conn: &Connection, id: i64) -> Res<SumSheet> {
    let (day, title, created_at): (i64, Option<String>, i64) = conn
        .query_row(
            "SELECT day, title, created_at FROM sum_sheet WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|_| "دفتر خوانده نشد".to_string())?
        .ok_or_else(|| "برگه پیدا نشد".to_string())?;
    let rows = rows_of(conn, id)?;
    let total = rows.iter().filter(|r| !r.struck).map(|r| r.amount).sum();
    Ok(SumSheet {
        id,
        day,
        title,
        created_at,
        rows,
        total,
    })
}

/// Newest sheet first — the top of the pile.
pub fn list(conn: &Connection) -> Res<Vec<SumSheet>> {
    let ids: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM sum_sheet ORDER BY day DESC, id DESC")
            .map_err(|_| "دفتر خوانده نشد".to_string())?;
        let found = stmt
            .query_map([], |r| r.get(0))
            .map_err(|_| "دفتر خوانده نشد".to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "دفتر خوانده نشد".to_string())?;
        found
    };
    ids.into_iter().map(|id| get(conn, id)).collect()
}

fn title_of(given: Option<&str>) -> Option<String> {
    given.map(str::trim).and_then(|title| {
        if title.is_empty() {
            None
        } else {
            Some(title.to_string())
        }
    })
}

/// Creates or rewrites a sheet. The rows that arrive are the sheet: old rows go,
/// `pos` follows the order they were sent in, and the total is whatever is left
/// after the struck lines.
pub fn save(conn: &Connection, input: &SumSheetInput) -> Res<SumSheet> {
    let old = match input.id {
        Some(id) => Some(get(conn, id)?),
        None => None,
    };

    let day = match input.day {
        Some(day) if day > 0 => day,
        Some(_) | None => old.as_ref().map(|s| s.day).unwrap_or_else(db::today),
    };
    let title = match input.title.as_deref() {
        Some(title) => title_of(Some(title)),
        None => old.as_ref().and_then(|s| s.title.clone()),
    };

    let tx = conn
        .unchecked_transaction()
        .map_err(|_| "ذخیره نشد".to_string())?;

    let (id, action) = match &old {
        Some(sheet) => {
            tx.execute(
                "UPDATE sum_sheet SET day = ?1, title = ?2 WHERE id = ?3",
                params![day, title, sheet.id],
            )
            .map_err(|_| "ذخیره نشد".to_string())?;
            (sheet.id, "update")
        }
        None => {
            tx.execute(
                "INSERT INTO sum_sheet(day, title, created_at) VALUES(?1, ?2, ?3)",
                params![day, title, db::now_ms()],
            )
            .map_err(|_| "ذخیره نشد".to_string())?;
            (tx.last_insert_rowid(), "create")
        }
    };

    tx.execute("DELETE FROM sum_row WHERE sheet_id = ?1", [id])
        .map_err(|_| "ذخیره نشد".to_string())?;

    let mut total = 0i64;
    for (pos, row) in input.rows.iter().enumerate() {
        let party_id = row.party_id.filter(|p| *p > 0);
        tx.execute(
            "INSERT INTO sum_row(sheet_id, pos, label, amount, party_id, struck)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id,
                pos as i64,
                row.label.trim(),
                row.amount,
                party_id,
                row.struck
            ],
        )
        .map_err(|_| "ذخیره نشد".to_string())?;
        if !row.struck {
            total += row.amount;
        }
    }

    db::audit(
        &tx,
        "sum_sheet",
        id,
        action,
        &json!({ "total": total, "rows": input.rows.len(), "day": day }).to_string(),
    )?;
    tx.commit().map_err(|_| "ذخیره نشد".to_string())?;

    get(conn, id)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveArgs {
    sheet: SumSheetInput,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdArgs {
    id: i64,
}

pub fn handle(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    Some(match op {
        "sums.list" => (|| {
            let conn = ctx.conn()?;
            Ok(serde_json::to_value(list(&conn)?).map_err(|_| "مشکلی پیش آمد".to_string())?)
        })(),
        "sums.get" => (|| {
            let a: IdArgs =
                serde_json::from_value(args.clone()).map_err(|_| "مشکلی پیش آمد".to_string())?;
            let conn = ctx.conn()?;
            Ok(serde_json::to_value(get(&conn, a.id)?).map_err(|_| "مشکلی پیش آمد".to_string())?)
        })(),
        "sums.save" => (|| {
            let a: SaveArgs =
                serde_json::from_value(args.clone()).map_err(|_| "مشکلی پیش آمد".to_string())?;
            let conn = ctx.conn()?;
            Ok(serde_json::to_value(save(&conn, &a.sheet)?)
                .map_err(|_| "مشکلی پیش آمد".to_string())?)
        })(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::SumRowInput;

    fn rows(values: &[(&str, i64, bool)]) -> Vec<SumRowInput> {
        values
            .iter()
            .map(|(label, amount, struck)| SumRowInput {
                id: None,
                label: label.to_string(),
                amount: *amount,
                party_id: None,
                struck: *struck,
            })
            .collect()
    }

    fn sheet_of(conn: &Connection, values: &[(&str, i64, bool)]) -> SumSheet {
        save(
            conn,
            &SumSheetInput {
                rows: rows(values),
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn struck_rows_do_not_count_and_un_striking_adds_them_back() {
        let conn = db::memory();
        let first = sheet_of(
            &conn,
            &[
                ("برنج", 100_000, false),
                ("روغن", 200_000, false),
                ("قند", 300_000, false),
            ],
        );
        assert_eq!(first.total, 600_000);
        assert_eq!(
            first.rows.iter().map(|r| r.pos).collect::<Vec<_>>(),
            vec![0, 1, 2],
            "pos follows the order the rows were sent in"
        );

        // Cross out the middle line: it must drop out of the total.
        let struck = save(
            &conn,
            &SumSheetInput {
                id: Some(first.id),
                rows: rows(&[
                    ("برنج", 100_000, false),
                    ("روغن", 200_000, true),
                    ("قند", 300_000, false),
                ]),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(struck.id, first.id, "the same sheet, rewritten");
        assert_eq!(struck.rows.len(), 3, "a struck line is still on the page");
        assert!(struck.rows[1].struck);
        assert_eq!(struck.total, 400_000);

        // Un-strike it and the total comes back.
        let back = save(
            &conn,
            &SumSheetInput {
                id: Some(first.id),
                rows: rows(&[
                    ("برنج", 100_000, false),
                    ("روغن", 200_000, false),
                    ("قند", 300_000, false),
                ]),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(back.total, 600_000);
        assert_eq!(
            list(&conn).unwrap().len(),
            1,
            "rewriting made no second sheet"
        );

        // Every rewrite left a line in the log.
        let logged: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM audit WHERE entity = 'sum_sheet' AND entity_id = ?1",
                [first.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(logged, 3);
    }

    #[test]
    fn a_sheet_keeps_its_date_its_title_and_its_place_in_the_pile() {
        let conn = db::memory();
        let older = save(
            &conn,
            &SumSheetInput {
                day: Some(20000),
                title: Some("برگه قدیمی".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        let newer = save(
            &conn,
            &SumSheetInput {
                title: Some("برگه امروز".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(newer.day, db::today(), "a new sheet is dated today");

        let sheets = list(&conn).unwrap();
        assert_eq!(sheets.len(), 2);
        assert_eq!(sheets[0].id, newer.id, "newest first");
        assert_eq!(sheets[1].id, older.id);

        let edited = save(
            &conn,
            &SumSheetInput {
                id: Some(older.id),
                day: Some(20001),
                title: Some("برگه اصلاح‌شده".to_string()),
                rows: rows(&[("نان", 25_000, false)]),
            },
        )
        .unwrap();
        assert_eq!(edited.id, older.id);
        assert_eq!(edited.day, 20001);
        assert_eq!(edited.title.as_deref(), Some("برگه اصلاح‌شده"));
        assert_eq!(edited.total, 25_000);
        assert_eq!(edited.created_at, older.created_at);

        // An empty row list is an empty sheet, not a missing one.
        let emptied = save(
            &conn,
            &SumSheetInput {
                id: Some(older.id),
                rows: Vec::new(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(emptied.rows.is_empty());
        assert_eq!(emptied.total, 0);
        assert_eq!(
            emptied.day, 20001,
            "a rewrite without a date keeps the old one"
        );

        assert!(get(&conn, 999).is_err(), "a missing sheet says so");
    }
}
