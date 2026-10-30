//! The message that travels with an invoice.
//!
//! The shopkeeper sends two things: the PDF page, and a short message written the
//! way he would type it himself, addressed to the customer by name. This module
//! writes only the words; where they go (Telegram, any other app through the
//! phone's own share sheet) is decided by the interface.

use crate::db::{self, Res};
use crate::domain::{balance, orders, parties, settings};
use crate::dto::Party;
use crate::jalali;
use crate::money::{fmt_currency, fmt_amount, fmt_qty};
use crate::Ctx;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;

/// «سلام فرزاد شریفی عزیز،» — a person's name, or a shop when the name is empty.
pub fn greeting(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        "سلام،".to_string()
    } else {
        format!("سلام {name} عزیز،")
    }
}

/// This party's documents that still have money on them, oldest first.
fn open_docs(conn: &Connection, party_id: i64) -> Res<Vec<crate::dto::DocRow>> {
    let sql = format!(
        "SELECT {}, p.name FROM doc d JOIN party p ON p.id = d.party_id
         WHERE d.party_id = ?1 AND d.status <> 'void'
         ORDER BY d.day ASC, d.id ASC",
        orders::DOC_COLUMNS
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| {
        eprintln!("share: {e}");
        "حساب‌ها خوانده نشد".to_string()
    })?;
    let rows = stmt
        .query_map([party_id], |r| {
            Ok(crate::dto::DocRow {
                doc: orders::from_row(r)?,
                party_name: r.get(16)?,
            })
        })
        .map_err(|e| {
            eprintln!("share: {e}");
            "حساب‌ها خوانده نشد".to_string()
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            eprintln!("share: {e}");
            "حساب‌ها خوانده نشد".to_string()
        })?;
    Ok(rows.into_iter().filter(|r| r.doc.remaining > 0).collect())
}

/// The whole message for one invoice: greeting, lines, totals, and what is left.
pub fn invoice_text(conn: &Connection, doc_id: i64) -> Res<String> {
    let doc = orders::load(conn, doc_id)?;
    let party: Party = parties::party_of(conn, doc.party_id)?;
    let lines = orders::lines_of(conn, doc_id)?;
    let s = settings::load(conn);

    let mut out = String::new();
    out.push_str(&greeting(&party.name));
    out.push('\n');

    let title = if doc.kind == "purchase" {
        format!("خرید شماره {} · {}", fmt_amount(doc.number), jalali::long(doc.day))
    } else {
        format!("فاکتور شماره {} · {}", fmt_amount(doc.number), jalali::long(doc.day))
    };
    out.push_str(&title);
    out.push('\n');

    // Only the lines that still count; a crossed-out line is not part of the bill
    // the customer is being asked to pay.
    let live: Vec<_> = lines.iter().filter(|l| !l.voided).collect();
    if !live.is_empty() {
        out.push('\n');
        for line in live {
            out.push_str(&format!(
                "• {} — {} × {} = {}\n",
                line.name,
                fmt_qty(line.qty_milli),
                fmt_amount(line.price),
                fmt_amount(line.total)
            ));
        }
    }

    out.push('\n');
    out.push_str(&format!("جمع کل: {}\n", fmt_currency(doc.total, &s.currency)));
    if doc.discount > 0 {
        out.push_str(&format!("تخفیف: {}\n", fmt_currency(doc.discount, &s.currency)));
    }
    if doc.paid > 0 {
        out.push_str(&format!("پرداخت شده: {}\n", fmt_currency(doc.paid, &s.currency)));
    }
    if doc.remaining > 0 {
        out.push_str(&format!("مانده: {}\n", fmt_currency(doc.remaining, &s.currency)));
    } else if doc.total > 0 {
        out.push_str("تسویه شده است.\n");
    }

    // How to pay, if the shop has told us.
    let card = s.card_number.trim();
    if !card.is_empty() {
        out.push('\n');
        if !s.card_holder.trim().is_empty() {
            out.push_str(&format!("به نام {}\n", s.card_holder.trim()));
        }
        if !s.bank_name.trim().is_empty() {
            out.push_str(&format!("{}\n", s.bank_name.trim()));
        }
        out.push_str(&format!("کارت: {card}\n"));
    }

    if !s.invoice_note.trim().is_empty() {
        out.push('\n');
        out.push_str(s.invoice_note.trim());
        out.push('\n');
    }

    Ok(out.trim_end().to_string())
}

/// The message that goes with a statement: the balance, in the words the
/// shopkeeper uses on the phone.
pub fn statement_text(conn: &Connection, party_id: i64) -> Res<String> {
    let party = parties::party_of(conn, party_id)?;
    let s = settings::load(conn);
    let bal = balance::of(conn, party_id)?;

    let mut out = String::new();
    out.push_str(&greeting(&party.name));
    out.push('\n');
    out.push_str("صورت‌حساب شما:\n\n");

    let open = open_docs(conn, party_id)?;
    if open.is_empty() {
        out.push_str("حسابی باز نیست.\n");
    } else {
        for row in open.iter().take(12) {
            let what = if row.doc.kind == "purchase" { "خرید" } else { "فاکتور" };
            out.push_str(&format!(
                "• {} {} · {} · مانده {}\n",
                what,
                fmt_amount(row.doc.number),
                jalali::short(row.doc.day),
                fmt_amount(row.doc.remaining)
            ));
        }
        out.push('\n');
    }

    if bal > 0 {
        out.push_str(&format!("مانده حساب: {}\n", fmt_currency(bal, &s.currency)));
        out.push_str("طلب ما از شماست.\n");
    } else if bal < 0 {
        out.push_str(&format!("مانده حساب: {}\n", fmt_currency(-bal, &s.currency)));
        out.push_str("طلب شما از ماست.\n");
    } else {
        out.push_str("حساب تسویه است. ممنون.\n");
    }

    if !s.invoice_note.trim().is_empty() {
        out.push('\n');
        out.push_str(s.invoice_note.trim());
        out.push('\n');
    }

    Ok(out.trim_end().to_string())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct DocArgs {
    doc_id: i64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PartyArgs {
    party_id: i64,
}

pub fn handle(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    Some(match op {
        "share.invoiceText" => (|| {
            let a: DocArgs = serde_json::from_value(args.clone())
                .map_err(|_| "شماره فاکتور خوانده نشد".to_string())?;
            let conn = ctx.conn()?;
            let text = invoice_text(&conn, a.doc_id)?;
            Ok(serde_json::json!({ "text": text, "title": "فاکتور" }))
        })(),
        "share.statementText" => (|| {
            let a: PartyArgs = serde_json::from_value(args.clone())
                .map_err(|_| "طرف حساب خوانده نشد".to_string())?;
            let conn = ctx.conn()?;
            let text = statement_text(&conn, a.party_id)?;
            let _ = db::today();
            Ok(serde_json::json!({ "text": text, "title": "صورت‌حساب" }))
        })(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded() -> Connection {
        let conn = crate::db::memory();
        conn.execute_batch(
            "INSERT INTO party(id, name, kind, opening, created_at, updated_at)
             VALUES(1, 'فرزاد شریفی', 'customer', 0, 0, 0);
             INSERT INTO doc(id, kind, jyear, number, party_id, day, status, discount, total, created_at, updated_at)
             VALUES(1, 'sale', 1404, 1, 1, 2460000, 'open', 0, 9_850_000, 0, 0);
             INSERT INTO doc_line(id, doc_id, pos, name, qty_milli, unit, price, total, voided)
             VALUES(1, 1, 1, 'خرما', 2000, 'کیلو', 2_000_000, 4_000_000, 0);
             INSERT INTO doc_line(id, doc_id, pos, name, qty_milli, unit, price, total, voided)
             VALUES(2, 1, 2, 'گردو', 3000, 'کیلو', 1_500_000, 4_500_000, 0);
             INSERT INTO doc_line(id, doc_id, pos, name, qty_milli, unit, price, total, voided)
             VALUES(3, 1, 3, 'لوبیا', 3000, 'کیلو', 450_000, 1_350_000, 1);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn the_message_starts_the_way_he_would_type_it() {
        let conn = seeded();
        let text = invoice_text(&conn, 1).unwrap();
        assert!(text.starts_with("سلام فرزاد شریفی عزیز،"), "{text}");
    }

    #[test]
    fn the_message_carries_the_lines_and_the_remainder() {
        let conn = seeded();
        let text = invoice_text(&conn, 1).unwrap();
        assert!(text.contains("خرما"), "{text}");
        assert!(text.contains("گردو"), "{text}");
        assert!(text.contains("جمع کل: ۹٬۸۵۰٬۰۰۰ تومان"), "{text}");
        assert!(text.contains("مانده: ۹٬۸۵۰٬۰۰۰ تومان"), "{text}");
    }

    #[test]
    fn a_crossed_out_line_is_not_part_of_the_message() {
        let conn = seeded();
        let text = invoice_text(&conn, 1).unwrap();
        assert!(!text.contains("لوبیا"), "a voided line must not be shown: {text}");
    }

    #[test]
    fn an_empty_name_still_greets_someone() {
        assert_eq!(greeting(""), "سلام،");
        assert_eq!(greeting("  لیلا  "), "سلام لیلا عزیز،");
    }
}
