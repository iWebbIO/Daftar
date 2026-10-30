//! The diary pages: what the shopkeeper writes down that is not money.
//!
//! Notes are read newest first, the way a notebook is flipped. Searching looks
//! through both the title and the body, and what is typed is taken literally —
//! a `%` in a note is a `%`, not a wildcard.

use crate::db::{self, Res};
use crate::dto::{Note, NoteInput};
use crate::Ctx;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::Value;

const COLUMNS: &str = "id, day, title, body, created_at, updated_at";
const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 500;

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note {
        id: r.get(0)?,
        day: r.get(1)?,
        title: r.get(2)?,
        body: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

pub fn get(conn: &Connection, id: i64) -> Res<Note> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM note WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()
    .map_err(|_| "دفتر خوانده نشد".to_string())?
    .ok_or_else(|| "یادداشت پیدا نشد".to_string())
}

/// `%`, `_` and `\` mean themselves when they are part of what was typed.
fn like_pattern(q: &str) -> String {
    let mut out = String::with_capacity(q.len() + 2);
    out.push('%');
    for c in q.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// Newest first. `q` looks in the title and in the body.
pub fn list(conn: &Connection, q: Option<&str>, limit: Option<i64>) -> Res<Vec<Note>> {
    let limit = limit
        .filter(|l| *l > 0)
        .unwrap_or(DEFAULT_LIMIT)
        .min(MAX_LIMIT);
    let pattern = q.map(str::trim).filter(|q| !q.is_empty()).map(like_pattern);

    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM note
             WHERE (?1 IS NULL OR title LIKE ?1 ESCAPE '\\' OR body LIKE ?1 ESCAPE '\\')
             ORDER BY day DESC, id DESC LIMIT ?2"
        ))
        .map_err(|_| "دفتر خوانده نشد".to_string())?;
    let rows = stmt
        .query_map(params![pattern, limit], from_row)
        .map_err(|_| "دفتر خوانده نشد".to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "دفتر خوانده نشد".to_string())?;
    Ok(rows)
}

/// A missing title keeps the old one; an empty title clears it. A missing day
/// keeps the old day, and a new page is dated today.

fn title_of(given: Option<&str>) -> Option<String> {
    given.map(str::trim).and_then(|title| {
        if title.is_empty() {
            None
        } else {
            Some(title.to_string())
        }
    })
}

pub fn save(conn: &Connection, input: &NoteInput) -> Res<Note> {
    let old = match input.id {
        Some(id) => Some(get(conn, id)?),
        None => None,
    };
    let now = db::now_ms();

    let day = match input.day {
        Some(day) if day > 0 => day,
        Some(_) => db::today(),
        None => old.as_ref().map(|n| n.day).unwrap_or_else(db::today),
    };
    let title = match input.title.as_deref() {
        Some(title) => title_of(Some(title)),
        None => old.as_ref().and_then(|n| n.title.clone()),
    };
    let body = input.body.clone();

    let id = match &old {
        Some(note) => {
            conn.execute(
                "UPDATE note SET day = ?1, title = ?2, body = ?3, updated_at = ?4 WHERE id = ?5",
                params![day, title, body, now, note.id],
            )
            .map_err(|_| "ذخیره نشد".to_string())?;
            note.id
        }
        None => {
            conn.execute(
                "INSERT INTO note(day, title, body, created_at, updated_at)
                 VALUES(?1, ?2, ?3, ?4, ?4)",
                params![day, title, body, now],
            )
            .map_err(|_| "ذخیره نشد".to_string())?;
            conn.last_insert_rowid()
        }
    };

    get(conn, id)
}

/// A diary page is nobody's money: deleting one takes nothing away from a
/// balance.
pub fn remove(conn: &Connection, id: i64) -> Res<bool> {
    let gone = conn
        .execute("DELETE FROM note WHERE id = ?1", [id])
        .map_err(|_| "پاک نشد".to_string())?;
    if gone == 0 {
        return Err("یادداشت پیدا نشد".to_string());
    }
    Ok(true)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListArgs {
    q: Option<String>,
    limit: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveArgs {
    note: NoteInput,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdArgs {
    id: i64,
}

pub fn handle(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    Some(match op {
        "notes.list" => (|| {
            let a: ListArgs =
                serde_json::from_value(args.clone()).map_err(|_| "مشکلی پیش آمد".to_string())?;
            let conn = ctx.conn()?;
            Ok(serde_json::to_value(list(&conn, a.q.as_deref(), a.limit)?)
                .map_err(|_| "مشکلی پیش آمد".to_string())?)
        })(),
        "notes.save" => (|| {
            let a: SaveArgs =
                serde_json::from_value(args.clone()).map_err(|_| "مشکلی پیش آمد".to_string())?;
            let conn = ctx.conn()?;
            Ok(serde_json::to_value(save(&conn, &a.note)?)
                .map_err(|_| "مشکلی پیش آمد".to_string())?)
        })(),
        "notes.remove" => (|| {
            let a: IdArgs =
                serde_json::from_value(args.clone()).map_err(|_| "مشکلی پیش آمد".to_string())?;
            let conn = ctx.conn()?;
            Ok(Value::Bool(remove(&conn, a.id)?))
        })(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_twice_updates_the_page_and_search_finds_it() {
        let conn = db::memory();

        let first = save(
            &conn,
            &NoteInput {
                title: Some("خرید برنج".to_string()),
                body: "امروز برنج خریدم".to_string(),
                day: Some(20000),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(first.created_at, first.updated_at);

        // The clock has to move for the bump to be visible.
        std::thread::sleep(std::time::Duration::from_millis(5));

        let second = save(
            &conn,
            &NoteInput {
                id: Some(first.id),
                body: "امروز برنج خریدم و پولش را دادم".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(second.id, first.id, "the page was updated, not doubled");
        assert_eq!(second.created_at, first.created_at, "it was not rewritten");
        assert!(second.updated_at > first.updated_at, "the edit is stamped");
        assert_eq!(second.day, 20000, "the date is kept when it is not sent");
        assert_eq!(second.title.as_deref(), Some("خرید برنج"));
        assert_eq!(list(&conn, None, None).unwrap().len(), 1);

        let found = list(&conn, Some("برنج"), None).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, first.id);
        assert!(found[0].body.contains("پولش"));

        // The title is searched too, and a word that is not there finds nothing.
        assert_eq!(list(&conn, Some("خرید"), None).unwrap().len(), 1);
        assert!(list(&conn, Some("گوجه"), None).unwrap().is_empty());
    }

    #[test]
    fn new_pages_are_dated_today_and_read_newest_first() {
        let conn = db::memory();
        let older = save(
            &conn,
            &NoteInput {
                body: "یادداشت قدیمی".to_string(),
                day: Some(20000),
                ..Default::default()
            },
        )
        .unwrap();
        let today = save(
            &conn,
            &NoteInput {
                body: "یادداشت امروز".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(today.day, db::today());

        let pages = list(&conn, None, None).unwrap();
        assert_eq!(pages[0].id, today.id);
        assert_eq!(pages[1].id, older.id);

        assert_eq!(list(&conn, None, Some(1)).unwrap().len(), 1);
        assert_eq!(
            list(&conn, None, Some(0)).unwrap().len(),
            2,
            "zero means the default, not nothing"
        );
    }

    #[test]
    fn a_wildcard_typed_in_a_note_is_just_a_character() {
        let conn = db::memory();
        save(
            &conn,
            &NoteInput {
                body: "۱۰۰٪ سود".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        save(
            &conn,
            &NoteInput {
                body: "هیچ چیز".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(list(&conn, Some("٪"), None).unwrap().len(), 1);
        assert_eq!(
            list(&conn, Some("%"), None).unwrap().len(),
            0,
            "a typed % is not «everything»"
        );
    }

    #[test]
    fn a_page_can_be_deleted() {
        let conn = db::memory();
        let note = save(
            &conn,
            &NoteInput {
                body: "موقت".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(remove(&conn, note.id).unwrap());
        assert!(list(&conn, None, None).unwrap().is_empty());
        assert!(remove(&conn, note.id).is_err(), "already gone");
    }
}
