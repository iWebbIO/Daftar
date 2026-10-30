//! Parties: people and shops, their balances and their book.
//!
//! One list, one balance, one page. Every number on a party comes from
//! `domain::balance`, so the list, the page and the home banner can never
//! disagree about what someone owes.

use crate::db::{self, Res};
use crate::domain::balance;
use crate::domain::orders::{self, in_tx};
use crate::domain::payments;
use crate::dto::{Party, PartyBook, PartyCard, PartyInput, TimelineItem, Totals};
use crate::money;
use crate::Ctx;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

pub const PARTY_COLUMNS: &str = "p.id, p.name, p.phone, p.kind, p.address, p.note, p.opening,
                                 p.archived, p.created_at, p.updated_at";

pub fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Party> {
    Ok(Party {
        id: r.get(0)?,
        name: r.get(1)?,
        phone: r.get(2)?,
        kind: r.get(3)?,
        address: r.get(4)?,
        note: r.get(5)?,
        opening: r.get(6)?,
        archived: r.get::<_, i64>(7)? != 0,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
    })
}

pub fn party_of(conn: &Connection, id: i64) -> Res<Party> {
    conn.query_row(
        &format!("SELECT {PARTY_COLUMNS} FROM party p WHERE p.id = ?1"),
        [id],
        from_row,
    )
    .optional()
    .map_err(|e| crate::db::oops("خواندن طرف حساب نشد", e))?
    .ok_or_else(|| "طرف حساب پیدا نشد".to_string())
}

/// The search text as a shopkeeper types it: any digit style, either yeh or kaf,
/// with or without the zero-width non-joiner, spaces or no spaces. «حاج اقا» and
/// «حاج‌آقا» are the same shop.
fn normalise(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_whitespace() {
            continue;
        }
        match c {
            '\u{200c}' | '\u{200d}' | '\u{200e}' | '\u{200f}' => continue,
            'ي' | 'ى' => out.push('ی'),
            'ك' => out.push('ک'),
            'ة' => out.push('ه'),
            'أ' | 'إ' | 'آ' | 'ٱ' => out.push('ا'),
            other => match money::digit_value(other) {
                Some(d) => out.push(char::from_digit(d, 10).unwrap_or(other)),
                None => out.extend(other.to_lowercase()),
            },
        }
    }
    out
}

fn last_days(conn: &Connection) -> Res<HashMap<i64, i64>> {
    let mut stmt = conn
        .prepare(
            "SELECT party_id, MAX(day) FROM (
                 SELECT party_id, day FROM doc WHERE status <> 'void'
                 UNION ALL
                 SELECT party_id, day FROM payment WHERE voided = 0
             ) GROUP BY party_id",
        )
        .map_err(|e| crate::db::oops("خواندن آخرین کار نشد", e))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
        .map_err(|e| crate::db::oops("خواندن آخرین کار نشد", e))?;
    let mut out = HashMap::new();
    for row in rows {
        let (id, day) = row.map_err(|e| crate::db::oops("خواندن آخرین کار نشد", e))?;
        out.insert(id, day);
    }
    Ok(out)
}

fn open_counts(conn: &Connection) -> Res<HashMap<i64, i64>> {
    let mut stmt = conn
        .prepare("SELECT party_id, COUNT(*) FROM doc WHERE status = 'open' GROUP BY party_id")
        .map_err(|e| crate::db::oops("خواندن فاکتورهای باز نشد", e))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
        .map_err(|e| crate::db::oops("خواندن فاکتورهای باز نشد", e))?;
    let mut out = HashMap::new();
    for row in rows {
        let (id, count) = row.map_err(|e| crate::db::oops("خواندن فاکتورهای باز نشد", e))?;
        out.insert(id, count);
    }
    Ok(out)
}

/// Every party with the numbers the list needs, in one pass: one balance query
/// for the whole book, one for the last activity, one for open invoices.
fn cards(conn: &Connection) -> Res<Vec<PartyCard>> {
    let balances = balance::all(conn)?;
    let last = last_days(conn)?;
    let open = open_counts(conn)?;
    let mut stmt = conn
        .prepare(&format!("SELECT {PARTY_COLUMNS} FROM party p ORDER BY p.name"))
        .map_err(|e| crate::db::oops("خواندن طرف حساب‌ها نشد", e))?;
    let rows = stmt
        .query_map([], from_row)
        .map_err(|e| crate::db::oops("خواندن طرف حساب‌ها نشد", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| crate::db::oops("خواندن طرف حساب‌ها نشد", e))?;
    Ok(rows
        .into_iter()
        .map(|party| PartyCard {
            balance: balances.get(&party.id).copied().unwrap_or(0),
            last_day: last.get(&party.id).copied(),
            open_docs: open.get(&party.id).copied().unwrap_or(0),
            party,
        })
        .collect())
}

fn matches(card: &PartyCard, needle: &str) -> bool {
    if normalise(&card.party.name).contains(needle) {
        return true;
    }
    card.party
        .phone
        .as_deref()
        .map(|phone| normalise(phone).contains(needle))
        .unwrap_or(false)
}

pub fn list(
    conn: &Connection,
    q: Option<&str>,
    filter: Option<&str>,
    sort: Option<&str>,
) -> Res<Vec<PartyCard>> {
    let mut cards = cards(conn)?;
    let needle = normalise(q.unwrap_or(""));
    if !needle.is_empty() {
        cards.retain(|card| matches(card, &needle));
    }
    match filter.unwrap_or("all") {
        "owes" => cards.retain(|card| card.balance > 0),
        "iowe" => cards.retain(|card| card.balance < 0),
        "settled" => cards.retain(|card| card.balance == 0),
        _ => {}
    }
    match sort.unwrap_or("balance") {
        "name" => cards.sort_by(|a, b| a.party.name.cmp(&b.party.name)),
        "recent" => cards.sort_by(|a, b| {
            b.last_day
                .unwrap_or(i64::MIN)
                .cmp(&a.last_day.unwrap_or(i64::MIN))
                .then_with(|| a.party.name.cmp(&b.party.name))
        }),
        "oldest" => cards.sort_by(|a, b| {
            a.last_day
                .unwrap_or(i64::MAX)
                .cmp(&b.last_day.unwrap_or(i64::MAX))
                .then_with(|| a.party.name.cmp(&b.party.name))
        }),
        _ => cards.sort_by(|a, b| {
            b.balance
                .cmp(&a.balance)
                .then_with(|| a.party.name.cmp(&b.party.name))
        }),
    }
    Ok(cards)
}

/// The first few names for the box the shopkeeper types in: what begins with the
/// letters typed comes first, most recently active before the rest.
pub fn suggest(conn: &Connection, q: &str) -> Res<Vec<PartyCard>> {
    let mut cards = cards(conn)?;
    cards.retain(|card| !card.party.archived);
    let needle = normalise(q);
    if !needle.is_empty() {
        cards.retain(|card| matches(card, &needle));
    }
    let begins = |card: &PartyCard| {
        if needle.is_empty() || normalise(&card.party.name).starts_with(&needle) {
            0
        } else {
            1
        }
    };
    cards.sort_by(|a, b| {
        begins(a)
            .cmp(&begins(b))
            .then_with(|| b.last_day.unwrap_or(i64::MIN).cmp(&a.last_day.unwrap_or(i64::MIN)))
            .then_with(|| a.party.name.cmp(&b.party.name))
    });
    cards.truncate(6);
    Ok(cards)
}

fn clean(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn save(conn: &Connection, input: &PartyInput) -> Res<Party> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err("نام را بنویس".to_string());
    }
    let kind = match input.kind.as_deref().unwrap_or("").trim() {
        "supplier" => "supplier",
        "both" => "both",
        _ => "customer",
    };
    let phone = clean(&input.phone);
    let address = clean(&input.address);
    let note = clean(&input.note);

    in_tx(conn, |conn| {
        let now = db::now_ms();
        let existing = input.id.filter(|id| *id > 0);
        let opening = match existing {
            Some(id) => input.opening.unwrap_or(party_of(conn, id)?.opening),
            None => input.opening.unwrap_or(0),
        };
        let archived = match existing {
            Some(id) => input.archived.unwrap_or(party_of(conn, id)?.archived),
            None => input.archived.unwrap_or(false),
        };
        let id = match existing {
            Some(id) => {
                party_of(conn, id)?;
                conn.execute(
                    "UPDATE party SET name = ?1, phone = ?2, kind = ?3, address = ?4, note = ?5,
                            opening = ?6, archived = ?7, updated_at = ?8
                     WHERE id = ?9",
                    params![
                        name,
                        phone,
                        kind,
                        address,
                        note,
                        opening,
                        archived as i64,
                        now,
                        id
                    ],
                )
                .map_err(|e| crate::db::oops("ذخیره طرف حساب نشد", e))?;
                db::audit(
                    conn,
                    "party",
                    id,
                    "update",
                    &json!({
                        "name": name,
                        "kind": kind,
                        "opening": opening,
                        "archived": archived,
                    })
                    .to_string(),
                )?;
                id
            }
            None => {
                conn.execute(
                    "INSERT INTO party(name, phone, kind, address, note, opening, archived,
                                       created_at, updated_at)
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        name,
                        phone,
                        kind,
                        address,
                        note,
                        opening,
                        archived as i64,
                        now,
                        now
                    ],
                )
                .map_err(|e| crate::db::oops("ذخیره طرف حساب نشد", e))?;
                let id = conn.last_insert_rowid();
                db::audit(
                    conn,
                    "party",
                    id,
                    "create",
                    &json!({"name": name, "kind": kind, "opening": opening}).to_string(),
                )?;
                id
            }
        };
        party_of(conn, id)
    })
}

/// A party's page: the balance, the documents, the payments, and the two of them
/// interleaved the way the notebook page reads, newest at the top.
pub fn book(conn: &Connection, id: i64) -> Res<PartyBook> {
    let party = party_of(conn, id)?;
    let balance = balance::of(conn, id)?;

    let docs = {
        let sql = format!(
            "SELECT {} FROM doc d
             WHERE d.party_id = ?1 AND d.status <> 'void'
             ORDER BY d.day DESC, d.id DESC",
            orders::DOC_COLUMNS
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| crate::db::oops("خواندن فاکتورها نشد", e))?;
        let rows = stmt
            .query_map([id], orders::from_row)
            .map_err(|e| crate::db::oops("خواندن فاکتورها نشد", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| crate::db::oops("خواندن فاکتورها نشد", e))?;
        rows
    };

    let paid = {
        let sql = format!(
            "SELECT {} FROM payment p
             WHERE p.party_id = ?1 AND p.voided = 0
             ORDER BY p.day DESC, p.id DESC",
            payments::PAYMENT_COLUMNS
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| crate::db::oops("خواندن پرداخت‌ها نشد", e))?;
        let rows = stmt
            .query_map([id], payments::from_row)
            .map_err(|e| crate::db::oops("خواندن پرداخت‌ها نشد", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| crate::db::oops("خواندن پرداخت‌ها نشد", e))?;
        rows
    };

    // (day, when it was written, documents before payments on a tie, item)
    let mut timeline: Vec<(i64, i64, u8, TimelineItem)> = Vec::new();
    for doc in &docs {
        let title = if doc.kind == "purchase" {
            format!("خرید {}", money::fa_digits(&doc.number.to_string()))
        } else {
            format!("فاکتور {}", money::fa_digits(&doc.number.to_string()))
        };
        timeline.push((
            doc.day,
            doc.created_at,
            0,
            TimelineItem {
                kind: doc.kind.clone(),
                id: doc.id,
                day: doc.day,
                title,
                amount: doc.total,
                remaining: doc.remaining,
                status: doc.status.clone(),
            },
        ));
    }
    for payment in &paid {
        let title = if payment.direction == "in" {
            "پول گرفتم".to_string()
        } else {
            "پول دادم".to_string()
        };
        timeline.push((
            payment.day,
            payment.created_at,
            1,
            TimelineItem {
                kind: "payment".to_string(),
                id: payment.id,
                day: payment.day,
                title,
                amount: payment.amount,
                remaining: 0,
                status: "settled".to_string(),
            },
        ));
    }
    timeline.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    let items = timeline.into_iter().map(|(_, _, _, item)| item).collect();

    Ok(PartyBook {
        party,
        balance,
        items,
        docs,
        payments: paid,
    })
}

pub fn totals(conn: &Connection) -> Res<Totals> {
    balance::totals(conn)
}

// ---------------------------------------------------------------------------
// the interface
// ---------------------------------------------------------------------------

fn read<T: serde::de::DeserializeOwned>(args: &Value) -> Res<T> {
    serde_json::from_value(args.clone()).map_err(|_| "ورودی خوانده نشد".to_string())
}

fn json_of<T: serde::Serialize>(value: T) -> Res<Value> {
    serde_json::to_value(value).map_err(|e| crate::db::oops("آماده کردن پاسخ نشد", e))
}

#[derive(Deserialize)]
struct IdArgs {
    id: i64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ListArgs {
    q: Option<String>,
    filter: Option<String>,
    sort: Option<String>,
}

#[derive(Deserialize)]
struct SaveArgs {
    party: PartyInput,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct SuggestArgs {
    q: Option<String>,
}

pub fn handle(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    Some(match op {
        "parties.list" => (|| {
            let a: ListArgs = read(args)?;
            let conn = ctx.conn()?;
            json_of(list(
                &conn,
                a.q.as_deref(),
                a.filter.as_deref(),
                a.sort.as_deref(),
            )?)
        })(),
        "parties.get" => (|| {
            let a: IdArgs = read(args)?;
            let conn = ctx.conn()?;
            json_of(party_of(&conn, a.id)?)
        })(),
        "parties.save" => (|| {
            let a: SaveArgs = read(args)?;
            let conn = ctx.conn()?;
            json_of(save(&conn, &a.party)?)
        })(),
        "parties.book" => (|| {
            let a: IdArgs = read(args)?;
            let conn = ctx.conn()?;
            json_of(book(&conn, a.id)?)
        })(),
        "parties.suggest" => (|| {
            let a: SuggestArgs = read(args)?;
            let conn = ctx.conn()?;
            json_of(suggest(&conn, a.q.as_deref().unwrap_or(""))?)
        })(),
        "parties.totals" => (|| {
            let conn = ctx.conn()?;
            json_of(totals(&conn)?)
        })(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::dto::{DocInput, LineInput, PaymentInput};
    use crate::jalali;

    fn day(n: i64) -> i64 {
        jalali::j2d(1404, 4, n)
    }

    fn add(conn: &Connection, name: &str, phone: Option<&str>) -> i64 {
        save(
            conn,
            &PartyInput {
                name: name.to_string(),
                phone: phone.map(str::to_string),
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

    fn pay(conn: &Connection, party_id: i64, direction: &str, amount: i64, on: i64) -> i64 {
        payments::save(
            conn,
            &PaymentInput {
                party_id,
                direction: direction.to_string(),
                amount,
                day: on,
                method: "cash".to_string(),
                ..Default::default()
            },
        )
        .unwrap()
        .id
    }

    #[test]
    fn the_list_says_who_owes_and_who_i_owe() {
        let conn = db::memory();
        let leila = add(&conn, "لیلا امینی", Some("09121234567"));
        let supplier = add(&conn, "حاج‌آقا", None);
        let settled = add(&conn, "مریم", None);
        doc(&conn, leila, "sale", day(9), 15_000, 345);
        doc(&conn, supplier, "purchase", day(10), 10_000, 1_000);

        let owes = list(&conn, None, Some("owes"), None).unwrap();
        assert_eq!(owes.len(), 1);
        assert_eq!(owes[0].party.id, leila);
        assert_eq!(owes[0].balance, 5_175);
        assert_eq!(owes[0].open_docs, 1);
        assert_eq!(owes[0].last_day, Some(day(9)));

        let iowe = list(&conn, None, Some("iowe"), None).unwrap();
        assert_eq!(iowe.len(), 1);
        assert_eq!(iowe[0].party.id, supplier);
        assert_eq!(iowe[0].balance, -10_000);

        let quiet = list(&conn, None, Some("settled"), None).unwrap();
        assert_eq!(quiet.len(), 1);
        assert_eq!(quiet[0].party.id, settled);

        let everyone = list(&conn, None, None, Some("name")).unwrap();
        assert_eq!(everyone.len(), 3);

        let t = totals(&conn).unwrap();
        assert_eq!(t.owes_me, 5_175);
        assert_eq!(t.i_owe, 10_000);
    }

    #[test]
    fn the_list_finds_a_name_or_a_phone_however_it_is_typed() {
        let conn = db::memory();
        add(&conn, "لیلا امینی", Some("09121234567"));
        add(&conn, "حاج‌آقا", Some("09351112233"));

        assert_eq!(list(&conn, Some("لیلا"), None, None).unwrap().len(), 1);
        assert_eq!(list(&conn, Some("امینی"), None, None).unwrap().len(), 1);
        // a phone typed with Persian digits still finds the Latin one
        let by_phone = list(&conn, Some("۰۹۱۲"), None, None).unwrap();
        assert_eq!(by_phone.len(), 1);
        assert_eq!(by_phone[0].party.name, "لیلا امینی");
        // and a name typed without the zero-width non-joiner
        assert_eq!(list(&conn, Some("حاج اقا"), None, None).unwrap().len(), 1);
        assert_eq!(list(&conn, Some("کسی"), None, None).unwrap().len(), 0);
    }

    #[test]
    fn the_party_page_reads_newest_first_with_persian_titles() {
        let conn = db::memory();
        let leila = add(&conn, "لیلا امینی", None);
        let doc_id = doc(&conn, leila, "sale", day(9), 15_000, 345);
        pay(&conn, leila, "in", 2_000, day(10));

        let page = book(&conn, leila).unwrap();
        assert_eq!(page.party.name, "لیلا امینی");
        assert_eq!(page.balance, 3_175);
        assert_eq!(page.docs.len(), 1);
        assert_eq!(page.payments.len(), 1);
        assert_eq!(page.docs[0].id, doc_id);
        assert_eq!(page.items.len(), 2);
        // newest first: the money came the day after the invoice
        assert_eq!(page.items[0].kind, "payment");
        assert_eq!(page.items[0].title, "پول گرفتم");
        assert_eq!(page.items[0].amount, 2_000);
        assert_eq!(page.items[1].kind, "sale");
        assert_eq!(page.items[1].title, "فاکتور ۱");
        assert_eq!(page.items[1].amount, 5_175);
        assert_eq!(page.items[1].remaining, 3_175);
        assert_eq!(page.items[1].status, "open");

        // a purchase reads differently and is still the same page
        let supplier = add(&conn, "حاج‌آقا", None);
        doc(&conn, supplier, "purchase", day(9), 10_000, 1_000);
        pay(&conn, supplier, "out", 4_000, day(9));
        let bought = book(&conn, supplier).unwrap();
        assert_eq!(bought.balance, -6_000);        assert!(bought
            .items
            .iter()
            .any(|item| item.title == "خرید ۱" && item.kind == "purchase"));
        assert!(bought.items.iter().any(|item| item.title == "پول دادم"));
    }

    #[test]
    fn suggest_finds_the_name_being_typed() {
        let conn = db::memory();
        let leila = add(&conn, "لیلا امینی", None);
        let laleh = add(&conn, "لاله", None);
        let reza = add(&conn, "رضا", None);
        // رضا was in the shop most recently, لیلا long ago
        doc(&conn, leila, "sale", day(1), 1_000, 100);
        doc(&conn, laleh, "sale", day(5), 1_000, 100);
        doc(&conn, reza, "sale", day(9), 1_000, 100);

        let typed = suggest(&conn, "لی").unwrap();
        assert_eq!(typed.len(), 1);
        assert_eq!(typed[0].party.id, leila);

        let both = suggest(&conn, "ل").unwrap();
        assert_eq!(both.len(), 2);
        assert_eq!(both[0].party.id, laleh, "most recently active first");
        assert_eq!(both[1].party.id, leila);

        let empty = suggest(&conn, "").unwrap();
        assert_eq!(empty.len(), 3);
        assert_eq!(empty[0].party.id, reza);

        // an archived party is not offered
        let archived = PartyInput {
            id: Some(reza),
            name: "رضا".to_string(),
            archived: Some(true),
            ..Default::default()
        };
        save(&conn, &archived).unwrap();
        let after = suggest(&conn, "").unwrap();
        assert_eq!(after.len(), 2);
    }

    #[test]
    fn a_party_is_a_name_and_nothing_else() {
        let conn = db::memory();
        let id = add(&conn, "  لیلا  ", None);
        let party = party_of(&conn, id).unwrap();
        assert_eq!(party.name, "لیلا", "trimmed");
        assert_eq!(party.kind, "customer");
        assert_eq!(party.opening, 0);
        assert!(!party.archived);

        let missing = PartyInput {
            name: "   ".to_string(),
            ..Default::default()
        };
        assert!(save(&conn, &missing).is_err());
        assert_eq!(
            party_of(&conn, 404).unwrap_err(),
            "طرف حساب پیدا نشد".to_string()
        );

        // an edit keeps the number, changes the words
        let edited = save(
            &conn,
            &PartyInput {
                id: Some(id),
                name: "لیلا امینی".to_string(),
                phone: Some("0912".to_string()),
                kind: Some("both".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(edited.id, id);
        assert_eq!(edited.name, "لیلا امینی");
        assert_eq!(edited.kind, "both");
        assert_eq!(edited.phone.as_deref(), Some("0912"));
    }

    #[test]
    fn the_party_interface_answers_its_own_names() {
        let state = memory_state();
        let ctx = Ctx { state: &state };
        let call = |op: &str, args: Value| handle(&ctx, op, &args).unwrap().unwrap();

        assert!(handle(&ctx, "parties.search", &json!({})).is_none());
        assert!(handle(&ctx, "orders.list", &json!({})).is_none());

        let saved: Party = serde_json::from_value(call(
            "parties.save",
            json!({"party": {"name": "لیلا امینی", "phone": "09121234567"}}),
        ))
        .unwrap();
        assert_eq!(saved.name, "لیلا امینی");
        assert_eq!(saved.kind, "customer");

        let cards: Vec<PartyCard> = serde_json::from_value(call(
            "parties.list",
            json!({"q": "لی", "filter": "settled", "sort": "name"}),
        ))
        .unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].balance, 0);
        assert_eq!(cards[0].party.id, saved.id);

        let one: Party =
            serde_json::from_value(call("parties.get", json!({"id": saved.id}))).unwrap();
        assert_eq!(one.id, saved.id);

        let page: PartyBook =
            serde_json::from_value(call("parties.book", json!({"id": saved.id}))).unwrap();
        assert_eq!(page.party.id, saved.id);
        assert_eq!(page.balance, 0);
        assert!(page.items.is_empty());
        assert!(page.docs.is_empty());

        let suggested: Vec<PartyCard> =
            serde_json::from_value(call("parties.suggest", json!({"q": "لی"}))).unwrap();
        assert_eq!(suggested.len(), 1);

        let totals: Totals = serde_json::from_value(call("parties.totals", json!({}))).unwrap();
        assert_eq!(totals.owes_me, 0);
        assert_eq!(totals.i_owe, 0);

        // a missing party is a short Persian sentence, never a database message
        assert_eq!(
            handle(&ctx, "parties.get", &json!({"id": 404}))
                .unwrap()
                .unwrap_err(),
            "طرف حساب پیدا نشد"
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
