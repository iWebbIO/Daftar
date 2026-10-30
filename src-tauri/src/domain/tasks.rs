//! Reminders: «به حسین بار بده», «شنبه پول برق».
//!
//! A task is a note to self, not a financial record, so it can be deleted. What
//! is open comes first; what has a date is read soonest first, and the undated
//! ones sit at the bottom where they belong.

use crate::db::{self, Res};
use crate::dto::{Task, TaskInput, TaskRow};
use crate::Ctx;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::Value;

const COLUMNS: &str = "t.id, t.text, t.day, t.party_id, t.done, t.created_at, p.name";

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRow> {
    Ok(TaskRow {
        task: Task {
            id: r.get(0)?,
            text: r.get(1)?,
            day: r.get(2)?,
            party_id: r.get(3)?,
            done: r.get(4)?,
            created_at: r.get(5)?,
        },
        party_name: r.get(6)?,
    })
}

fn one(conn: &Connection, id: i64) -> Res<TaskRow> {
    conn.query_row(
        &format!(
            "SELECT {COLUMNS} FROM task t LEFT JOIN party p ON p.id = t.party_id WHERE t.id = ?1"
        ),
        [id],
        from_row,
    )
    .optional()
    .map_err(|e| format!("خواندن کار نشد: {e}"))?
    .ok_or_else(|| "این کار پیدا نشد".to_string())
}

/// Open first, then by day, undated last.
pub fn list(conn: &Connection, include_done: bool) -> Res<Vec<TaskRow>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM task t LEFT JOIN party p ON p.id = t.party_id
             WHERE (?1 = 1 OR t.done = 0)
             ORDER BY t.done ASC,
                      CASE WHEN t.day IS NULL THEN 1 ELSE 0 END ASC,
                      t.day ASC, t.id ASC"
        ))
        .map_err(|e| format!("خواندن کارها نشد: {e}"))?;
    let rows = stmt
        .query_map([include_done], from_row)
        .map_err(|e| format!("خواندن کارها نشد: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("خواندن کارها نشد: {e}"))?;
    Ok(rows)
}

/// `day: 0` and `partyId: 0` mean «no date» and «no party»; leaving them out
/// on an edit keeps what was there.

fn ensure_party(conn: &Connection, party_id: i64) -> Res<()> {
    let known: Option<i64> = conn
        .query_row("SELECT id FROM party WHERE id = ?1", [party_id], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|e| format!("خواندن طرف حساب نشد: {e}"))?;
    if known.is_none() {
        return Err("این طرف حساب پیدا نشد".to_string());
    }
    Ok(())
}

pub fn save(conn: &Connection, input: &TaskInput) -> Res<Task> {
    let old = match input.id {
        Some(id) => Some(one(conn, id)?.task),
        None => None,
    };

    let text = input.text.trim();
    if text.is_empty() {
        return Err("متن کار را بنویسید".to_string());
    }

    let day = match input.day {
        Some(day) if day > 0 => Some(day),
        Some(_) => None,
        None => old.as_ref().and_then(|t| t.day),
    };
    let party_id = match input.party_id {
        Some(party_id) if party_id > 0 => {
            ensure_party(conn, party_id)?;
            Some(party_id)
        }
        Some(_) => None,
        None => old.as_ref().and_then(|t| t.party_id),
    };
    let done = input
        .done
        .unwrap_or_else(|| old.as_ref().map(|t| t.done).unwrap_or(false));

    let id = match &old {
        Some(task) => {
            conn.execute(
                "UPDATE task SET text = ?1, day = ?2, party_id = ?3, done = ?4 WHERE id = ?5",
                params![text, day, party_id, done, task.id],
            )
            .map_err(|e| format!("ذخیره کار نشد: {e}"))?;
            task.id
        }
        None => {
            conn.execute(
                "INSERT INTO task(text, day, party_id, done, created_at) VALUES(?1, ?2, ?3, ?4, ?5)",
                params![text, day, party_id, done, db::now_ms()],
            )
            .map_err(|e| format!("ذخیره کار نشد: {e}"))?;
            conn.last_insert_rowid()
        }
    };

    Ok(one(conn, id)?.task)
}

pub fn toggle(conn: &Connection, id: i64) -> Res<Task> {
    let task = one(conn, id)?.task;
    conn.execute(
        "UPDATE task SET done = ?1 WHERE id = ?2",
        params![!task.done, id],
    )
    .map_err(|e| format!("تغییر کار نشد: {e}"))?;
    Ok(one(conn, id)?.task)
}

/// A reminder is not money: deleting it is honest, not a lost record.
pub fn remove(conn: &Connection, id: i64) -> Res<bool> {
    let gone = conn
        .execute("DELETE FROM task WHERE id = ?1", [id])
        .map_err(|e| format!("پاک کردن کار نشد: {e}"))?;
    if gone == 0 {
        return Err("این کار پیدا نشد".to_string());
    }
    Ok(true)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListArgs {
    include_done: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveArgs {
    task: TaskInput,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdArgs {
    id: i64,
}

pub fn handle(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    Some(match op {
        "tasks.list" => (|| {
            let a: ListArgs = serde_json::from_value(args.clone())
                .map_err(|_| "فهرست کارها خوانده نشد".to_string())?;
            let conn = ctx.conn()?;
            Ok(
                serde_json::to_value(list(&conn, a.include_done.unwrap_or(false))?)
                    .map_err(|e| format!("خواندن کارها نشد: {e}"))?,
            )
        })(),
        "tasks.save" => (|| {
            let a: SaveArgs = serde_json::from_value(args.clone())
                .map_err(|_| "اطلاعات کار خوانده نشد".to_string())?;
            let conn = ctx.conn()?;
            Ok(serde_json::to_value(save(&conn, &a.task)?)
                .map_err(|e| format!("ذخیره کار نشد: {e}"))?)
        })(),
        "tasks.toggle" => (|| {
            let a: IdArgs =
                serde_json::from_value(args.clone()).map_err(|_| "کار پیدا نشد".to_string())?;
            let conn = ctx.conn()?;
            Ok(serde_json::to_value(toggle(&conn, a.id)?)
                .map_err(|e| format!("تغییر کار نشد: {e}"))?)
        })(),
        "tasks.remove" => (|| {
            let a: IdArgs =
                serde_json::from_value(args.clone()).map_err(|_| "کار پیدا نشد".to_string())?;
            let conn = ctx.conn()?;
            Ok(Value::Bool(remove(&conn, a.id)?))
        })(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO party(id, name, kind, opening, created_at, updated_at)
             VALUES(1, 'حسین', 'customer', 0, 0, 0);",
        )
        .unwrap();
    }

    fn new_task(conn: &Connection, text: &str, day: Option<i64>) -> Task {
        save(
            conn,
            &TaskInput {
                text: text.to_string(),
                day,
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn open_tasks_come_first_and_undated_ones_last() {
        let conn = db::memory();
        seed(&conn);

        let undated = new_task(&conn, "به حسین بار بده", None);
        let later = new_task(&conn, "شنبه پول برق", Some(21000));
        let sooner = new_task(&conn, "فردا نان بگیر", Some(20000));
        let finished = save(
            &conn,
            &TaskInput {
                text: "سبد خرید".to_string(),
                day: Some(19999),
                done: Some(true),
                ..Default::default()
            },
        )
        .unwrap();

        let open = list(&conn, false).unwrap();
        assert_eq!(open.len(), 3, "done work is not on the list");
        let texts: Vec<&str> = open.iter().map(|r| r.task.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["فردا نان بگیر", "شنبه پول برق", "به حسین بار بده"]
        );

        let all = list(&conn, true).unwrap();
        assert_eq!(all.len(), 4);
        assert_eq!(all.last().unwrap().task.id, finished.id, "done work last");
        assert!(all.last().unwrap().task.done);
        assert_eq!(all[0].task.id, sooner.id);
        assert_eq!(all[2].task.id, undated.id);
        assert!(later.day.is_some());
    }

    #[test]
    fn a_reminder_can_be_written_linked_ticked_and_deleted() {
        let conn = db::memory();
        seed(&conn);

        let task = save(
            &conn,
            &TaskInput {
                text: "  به حسین بار بده  ".to_string(),
                party_id: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(task.text, "به حسین بار بده", "text is trimmed");
        assert!(!task.done);

        let listed = list(&conn, true).unwrap();
        assert_eq!(listed[0].party_name.as_deref(), Some("حسین"));

        let ticked = toggle(&conn, task.id).unwrap();
        assert!(ticked.done);
        let unticked = toggle(&conn, task.id).unwrap();
        assert!(!unticked.done);

        assert!(remove(&conn, task.id).unwrap());
        assert!(list(&conn, true).unwrap().is_empty());
        assert!(remove(&conn, task.id).is_err(), "already gone");
    }

    #[test]
    fn an_empty_reminder_is_refused() {
        let conn = db::memory();
        seed(&conn);
        let blank = save(
            &conn,
            &TaskInput {
                text: "   ".to_string(),
                ..Default::default()
            },
        );
        assert!(blank.is_err());
        let missing = save(&conn, &TaskInput::default());
        assert!(missing.is_err());
    }
}
