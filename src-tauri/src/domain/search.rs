//! One search over names, products, amounts and note text.
//!
//! He has one box at the top of the screen and types whatever he remembers: a
//! customer's name, part of a phone number, «نخود», the number of an invoice or
//! the amount written on it — «۵٬۱۷۵», `5175`, «5,175» are the same number.

use crate::db::Res;
use crate::domain::products::{digits_of, fail, normalise};
use crate::dto::SearchHit;
use crate::jalali;
use crate::money;
use crate::Ctx;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;

/// One screen of results is enough for one shop.
const MAX_HITS: usize = 40;
/// A single letter is not a search, it is the whole book.
const MIN_CHARS: usize = 2;

/// How good a match is. Smaller is better.
const EXACT: i64 = 0;
const STARTS: i64 = 1;
const INSIDE: i64 = 2;
/// An archived party or a retired product is still findable, just not first.
const DEMOTED: i64 = 10;

/// People first, then goods, then paper, then scribbles.
const RANK_PARTY: i64 = 0;
const RANK_PRODUCT: i64 = 1;
const RANK_DOC: i64 = 2;
const RANK_NOTE: i64 = 3;

struct Found {
    score: i64,
    rank: i64,
    /// Bigger is newer. Only compared between things of the same rank.
    recency: i64,
    hit: SearchHit,
}

/// How well `text` answers `needle`, which is already normalised.
fn how_well(text: &str, needle: &str) -> Option<i64> {
    let folded = normalise(text);
    if folded == needle {
        Some(EXACT)
    } else if folded.starts_with(needle) {
        Some(STARTS)
    } else if folded.contains(needle) {
        Some(INSIDE)
    } else {
        None
    }
}

fn best(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// The amount he typed. A Persian keyboard puts «،» where a comma goes and «٬»
/// where a thousand goes; `money::parse_amount` wants a plain comma.
fn typed_amount(q: &str) -> Option<i64> {
    let plain: String = q
        .chars()
        .map(|c| match c {
            '\u{060C}' | '\u{066C}' => ',',
            c => c,
        })
        .collect();
    money::parse_amount(&plain)
}

pub fn all(conn: &Connection, q: &str) -> Res<Vec<SearchHit>> {
    let needle = normalise(q);
    if needle.chars().count() < MIN_CHARS {
        return Ok(Vec::new());
    }
    let digits = digits_of(q);
    let amount = typed_amount(q);

    let mut found: Vec<Found> = Vec::new();
    parties(conn, &needle, &digits, &mut found)?;
    products(conn, &needle, &mut found)?;
    documents(conn, &digits, amount, &mut found)?;
    notes(conn, &needle, &mut found)?;

    found.sort_by(|a, b| {
        a.score
            .cmp(&b.score)
            .then(a.rank.cmp(&b.rank))
            .then(b.recency.cmp(&a.recency))
            .then(a.hit.id.cmp(&b.hit.id))
    });
    found.truncate(MAX_HITS);
    Ok(found.into_iter().map(|f| f.hit).collect())
}

fn parties(conn: &Connection, needle: &str, digits: &str, out: &mut Vec<Found>) -> Res<()> {
    let mut stmt = conn
        .prepare("SELECT id, name, phone, archived, updated_at FROM party")
        .map_err(|e| fail("جستجوی مشتری‌ها نشد", e))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })
        .map_err(|e| fail("جستجوی مشتری‌ها نشد", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| fail("جستجوی مشتری‌ها نشد", e))?;

    for (id, name, phone, archived, updated_at) in rows {
        let by_name = how_well(&name, needle);
        let by_phone = phone
            .as_deref()
            .filter(|_| !digits.is_empty())
            .and_then(|p| digits_of(p).contains(digits).then_some(STARTS));
        let Some(mut score) = best(by_name, by_phone) else { continue };
        if archived != 0 {
            score += DEMOTED;
        }
        out.push(Found {
            score,
            rank: RANK_PARTY,
            recency: updated_at,
            hit: SearchHit {
                kind: "party".to_string(),
                id,
                title: name,
                subtitle: phone.unwrap_or_else(|| "بدون شماره".to_string()),
                amount: None,
            },
        });
    }
    Ok(())
}

fn products(conn: &Connection, needle: &str, out: &mut Vec<Found>) -> Res<()> {
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.name, p.unit, p.updated_at,
                    COALESCE((SELECT ph.price FROM price_history ph
                              WHERE ph.product_id = p.id AND ph.kind = 'sale'
                              ORDER BY ph.at DESC, ph.id DESC LIMIT 1), p.sale_price),
                    p.active
             FROM product p",
        )
        .map_err(|e| fail("جستجوی کالاها نشد", e))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })
        .map_err(|e| fail("جستجوی کالاها نشد", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| fail("جستجوی کالاها نشد", e))?;

    for (id, name, unit, updated_at, last_price, active) in rows {
        let Some(mut score) = how_well(&name, needle) else { continue };
        if active == 0 {
            score += DEMOTED;
        }
        out.push(Found {
            score,
            rank: RANK_PRODUCT,
            recency: updated_at,
            hit: SearchHit {
                kind: "product".to_string(),
                id,
                title: name,
                subtitle: product_subtitle(&unit, last_price),
                amount: None,
            },
        });
    }
    Ok(())
}

/// «کیلو · آخرین قیمت ۳۴۵٬۰۰۰», or just the unit for something never priced.
fn product_subtitle(unit: &str, last_price: i64) -> String {
    if last_price > 0 {
        format!("{} · آخرین قیمت {}", unit, money::fmt_amount(last_price))
    } else {
        unit.to_string()
    }
}

fn documents(
    conn: &Connection,
    digits: &str,
    amount: Option<i64>,
    out: &mut Vec<Found>,
) -> Res<()> {
    let number_wanted = digits.parse::<i64>().ok();
    let mut stmt = conn
        .prepare(
            "SELECT d.id, d.kind, d.number, d.day, d.total, p.name
             FROM doc d JOIN party p ON p.id = d.party_id
             WHERE d.status <> 'void'",
        )
        .map_err(|e| fail("جستجوی فاکتورها نشد", e))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
            ))
        })
        .map_err(|e| fail("جستجوی فاکتورها نشد", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| fail("جستجوی فاکتورها نشد", e))?;

    for (id, kind, number, day, total, party_name) in rows {
        let by_number = match number_wanted {
            Some(wanted) if wanted == number => Some(EXACT),
            Some(_) if !digits.is_empty() && number.to_string().starts_with(digits) => Some(STARTS),
            _ => None,
        };
        // He types the amount written in the notebook, not the line math.
        let by_amount = match amount {
            Some(wanted) if wanted == total => Some(EXACT),
            _ => None,
        };
        let Some(score) = best(by_number, by_amount) else { continue };

        let word = if kind == "purchase" { "خرید" } else { "فروش" };
        out.push(Found {
            score,
            rank: RANK_DOC,
            recency: day,
            hit: SearchHit {
                kind,
                id,
                title: format!("{word} شماره {}", money::fa_digits(&number.to_string())),
                subtitle: format!("{} · {}", party_name, jalali::long(day)),
                amount: Some(total),
            },
        });
    }
    Ok(())
}

fn notes(conn: &Connection, needle: &str, out: &mut Vec<Found>) -> Res<()> {
    let mut stmt = conn
        .prepare("SELECT id, title, body, day FROM note")
        .map_err(|e| fail("جستجوی یادداشت‌ها نشد", e))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })
        .map_err(|e| fail("جستجوی یادداشت‌ها نشد", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| fail("جستجوی یادداشت‌ها نشد", e))?;

    for (id, title, body, day) in rows {
        let by_title = title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .and_then(|t| how_well(t, needle));
        let by_body = (!body.trim().is_empty())
            .then(|| how_well(&body, needle))
            .flatten();
        let Some(score) = best(by_title, by_body) else { continue };

        out.push(Found {
            score,
            rank: RANK_NOTE,
            recency: day,
            hit: SearchHit {
                kind: "note".to_string(),
                id,
                title: note_title(title.as_deref(), &body),
                subtitle: jalali::long(day),
                amount: None,
            },
        });
    }
    Ok(())
}

fn note_title(title: Option<&str>, body: &str) -> String {
    if let Some(title) = title.map(str::trim).filter(|t| !t.is_empty()) {
        return title.chars().take(48).collect();
    }
    let first = body
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let short: String = first.chars().take(40).collect();
    if short.is_empty() {
        "یادداشت".to_string()
    } else if first.chars().count() > 40 {
        format!("{short}…")
    } else {
        short
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchArgs {
    #[serde(default)]
    q: String,
}

pub fn handle(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    Some(match op {
        "search.all" => (|| -> Res<Value> {
            let a: SearchArgs = serde_json::from_value(args.clone())
                .unwrap_or(SearchArgs { q: String::new() });
            let conn = ctx.conn()?;
            let hits = all(&conn, &a.q)?;
            serde_json::to_value(hits).map_err(|e| fail("نتیجه جستجو ساخته نشد", e))
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
    const RICE: &str = "\u{628}\u{631}\u{646}\u{62C}";

    fn add_party(conn: &Connection, id: i64, name: &str, phone: Option<&str>) {
        conn.execute(
            "INSERT INTO party(id, name, phone, kind, created_at, updated_at)
             VALUES(?1, ?2, ?3, 'customer', 0, 0)",
            rusqlite::params![id, name, phone],
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

    fn add_doc(conn: &Connection, id: i64, kind: &str, number: i64, day: i64, status: &str, total: i64) {
        conn.execute(
            "INSERT INTO doc(id, kind, jyear, number, party_id, day, status, total, created_at, updated_at)
             VALUES(?1, ?2, 1404, ?3, 1, ?4, ?5, ?6, 0, 0)",
            rusqlite::params![id, kind, number, day, status, total],
        )
        .unwrap();
    }

    fn seeded() -> Connection {
        let conn = db::memory();
        add_party(&conn, 1, "لیلا امینی", Some("0912 345 6789"));
        add_product(&conn, 5, NOK, "کیلو");
        add_doc(&conn, 1, "sale", 12, 20_000, "open", 5_175);
        conn.execute(
            "INSERT INTO price_history(product_id, party_id, kind, price, day, at)
             VALUES(5, NULL, 'sale', 345000, 20000, 1000)",
            [],
        )
        .unwrap();
        conn
    }

    /// Test 6: the same amount typed three ways finds the same invoice.
    #[test]
    fn finds_an_invoice_by_its_amount_in_any_digit_style() {
        let conn = seeded();

        for q in ["۵٬۱۷۵", "5175", "5,175", "۵،۱۷۵"] {
            let hits = all(&conn, q).unwrap();
            assert!(
                hits.iter().any(|h| h.kind == "sale" && h.id == 1),
                "«{q}» should find the invoice, got {hits:?}"
            );
            let hit = hits.iter().find(|h| h.kind == "sale").unwrap();
            assert_eq!(hit.amount, Some(5_175));
            assert_eq!(hit.title, "فروش شماره ۱۲");
            assert_eq!(
                hit.subtitle,
                format!("لیلا امینی · {}", jalali::long(20_000)),
                "the party and the day, in Jalali"
            );
        }

        // A cancelled invoice is not offered, and neither is an amount nobody wrote.
        add_doc(&conn, 2, "sale", 44, 20_001, "void", 9_999);
        assert!(all(&conn, "9999").unwrap().is_empty());
        assert!(all(&conn, "8888").unwrap().is_empty());
    }

    #[test]
    fn finds_a_document_by_its_number() {
        let conn = seeded();
        let hits = all(&conn, "۱۲").unwrap();
        // The invoice comes first; the phone number that also contains «12» follows.
        assert_eq!(hits[0].kind, "sale");
        assert_eq!(hits[0].id, 1);
        assert_eq!(hits[0].amount, Some(5_175));
    }

    /// Test 7: an empty or one-letter box is not a search.
    #[test]
    fn an_empty_query_finds_nothing() {
        let conn = seeded();
        add_party(&conn, 2, "ننه جان", None);
        assert!(all(&conn, "").unwrap().is_empty());
        assert!(all(&conn, "   ").unwrap().is_empty());
        assert!(all(&conn, "ن").unwrap().is_empty(), "one letter is everything");
        assert!(all(&conn, "۵").unwrap().is_empty(), "one digit is everything");
    }

    #[test]
    fn finds_a_product_with_its_last_price() {
        let conn = seeded();
        let hits = all(&conn, NOK).unwrap();
        let hit = hits.iter().find(|h| h.kind == "product").unwrap();
        assert_eq!(hit.id, 5);
        assert_eq!(hit.title, NOK);
        assert_eq!(hit.amount, None);
        assert_eq!(
            hit.subtitle,
            format!("کیلو · آخرین قیمت {}", money::fmt_amount(345_000))
        );
    }

    #[test]
    fn finds_a_party_by_phone_and_a_note_by_its_words() {
        let conn = seeded();
        // A phone number typed in any digit style.
        for q in ["0912", "۰۹۱۲"] {
            let hits = all(&conn, q).unwrap();
            assert_eq!(hits.len(), 1, "«{q}» should find the customer");
            assert_eq!(hits[0].kind, "party");
            assert_eq!(hits[0].title, "لیلا امینی");
        }

        conn.execute(
            "INSERT INTO note(id, day, title, body, created_at, updated_at)
             VALUES(1, 20001, NULL, 'برنج برای حاج آقا سفارش شود', 0, 0)",
            [],
        )
        .unwrap();
        let hits = all(&conn, RICE).unwrap();
        let hit = hits.iter().find(|h| h.kind == "note").unwrap();
        assert_eq!(hit.id, 1);
        assert_eq!(hit.title, "برنج برای حاج آقا سفارش شود");
        assert_eq!(hit.subtitle, jalali::long(20_001));
        assert_eq!(hit.amount, None);

        // A note is also found by a word from the middle of its body and by its title.
        assert!(all(&conn, "حاج آقا").unwrap().iter().any(|h| h.kind == "note"));
    }

    #[test]
    fn handle_answers_only_its_own_names() {
        let state = crate::AppState {
            db: std::sync::Mutex::new(seeded()),
            dirs: crate::Dirs {
                data: std::path::PathBuf::new(),
                files: std::path::PathBuf::new(),
            },
        };
        let ctx = Ctx { state: &state };
        assert!(handle(&ctx, "search.all", &serde_json::json!({ "q": "5175" })).is_some());
        assert!(handle(&ctx, "products.list", &serde_json::json!({})).is_none());

        let found = handle(&ctx, "search.all", &serde_json::json!({ "q": "5175" }))
            .unwrap()
            .unwrap();
        let hits: Vec<SearchHit> = serde_json::from_value(found).unwrap();
        assert_eq!(hits[0].kind, "sale");
    }
}
