//! Products, their units and their price memory.
//!
//! The whole point of this module is that the shopkeeper never types the same
//! price twice. Every price he has ever typed lands in `price_history`, the list
//! puts whatever he uses most at the top, and the order screen can ask what
//! *this* customer actually buys.

use crate::db::{self, Res};
use crate::dto::{PricePoint, Product, ProductInput};
use crate::money;
use crate::Ctx;
use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::Value;

/// Everything is sold by weight until he says otherwise.
pub const DEFAULT_UNIT: &str = "کیلو";

/// How much price history is worth showing.
const HISTORY_MAX: usize = 40;
/// How many of a customer's usual items the order screen offers.
const USUAL_MAX: usize = 12;

/// A short Persian sentence for the shopkeeper, with the technical detail kept
/// on the developer's side of the fence.
pub(crate) fn fail(what: &str, e: impl std::fmt::Display) -> String {
    eprintln!("daftar · {what}: {e}");
    what.to_string()
}

/// Persian text as it should be compared.
///
/// Two people type the same word differently, and a phone keyboard adds its own
/// habits: ي instead of ی, ك instead of ک, a ZWNJ where a word is joined. This
/// folds all of that away so «نخود», «نخود» and «نخود» are one word. Latin is
/// lowercased and every digit style becomes an ASCII digit.
pub fn normalise(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            // A zero-width joiner is a typing habit, not a letter.
            '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}' => continue,
            '\u{064A}' | '\u{0649}' | '\u{06D0}' => folded.push('\u{06CC}'), // ي ى ې -> ی
            '\u{0643}' => folded.push('\u{06A9}'),                           // ك -> ک
            '\u{0622}' | '\u{0623}' | '\u{0625}' | '\u{0671}' => folded.push('\u{0627}'), // آ أ إ -> ا
            '\u{0629}' | '\u{06C0}' => folded.push('\u{0647}'),              // ة ۀ -> ه
            c if c.is_whitespace() => folded.push(' '),
            c => match money::digit_value(c) {
                Some(d) => folded.push(char::from_digit(d, 10).unwrap_or(c)),
                None => folded.push(c),
            },
        }
    }

    let mut clean = String::with_capacity(folded.len());
    let mut after_space = true;
    for c in folded.chars() {
        if c == ' ' {
            if !after_space {
                clean.push(' ');
                after_space = true;
            }
        } else {
            clean.push(c);
            after_space = false;
        }
    }
    clean.trim().to_lowercase()
}

/// Only the digits of a string, in ASCII, so two phone numbers typed on two
/// keyboards still look alike.
pub fn digits_of(text: &str) -> String {
    normalise(text).chars().filter(char::is_ascii_digit).collect()
}

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Product> {
    Ok(Product {
        id: r.get(0)?,
        name: r.get(1)?,
        unit: r.get(2)?,
        sale_price: r.get(3)?,
        buy_price: r.get(4)?,
        active: r.get::<_, i64>(5)? != 0,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
    })
}

pub fn load(conn: &Connection, id: i64) -> Res<Product> {
    conn.query_row(
        "SELECT id, name, unit, sale_price, buy_price, active, created_at, updated_at
         FROM product WHERE id = ?1",
        [id],
        from_row,
    )
    .optional()
    .map_err(|e| fail("کالا خوانده نشد", e))?
    .ok_or_else(|| "کالا پیدا نشد".to_string())
}

/// The same product under another spelling, if he has already saved it once.
fn by_name(conn: &Connection, name: &str) -> Res<Option<Product>> {
    let needle = normalise(name);
    let mut stmt = conn
        .prepare(
            "SELECT id, name, unit, sale_price, buy_price, active, created_at, updated_at
             FROM product",
        )
        .map_err(|e| fail("کالا خوانده نشد", e))?;
    let all = stmt
        .query_map([], from_row)
        .map_err(|e| fail("کالا خوانده نشد", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| fail("کالا خوانده نشد", e))?;
    Ok(all.into_iter().find(|p| normalise(&p.name) == needle))
}

/// The list the product picker shows: what he uses most, first.
pub fn list(conn: &Connection, q: Option<&str>) -> Res<Vec<Product>> {
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.name, p.unit, p.sale_price, p.buy_price, p.active,
                    p.created_at, p.updated_at,
                    COALESCE((SELECT MAX(ph.at) FROM price_history ph
                              WHERE ph.product_id = p.id), 0) AS used
             FROM product p",
        )
        .map_err(|e| fail("فهرست کالاها خوانده نشد", e))?;
    let mut rows = stmt
        .query_map([], |r| Ok((from_row(r)?, r.get::<_, i64>(8)?)))
        .map_err(|e| fail("فهرست کالاها خوانده نشد", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| fail("فهرست کالاها خوانده نشد", e))?;

    let needle = q.map(normalise).unwrap_or_default();
    if !needle.is_empty() {
        rows.retain(|(p, _)| normalise(&p.name).contains(&needle));
    }

    // Active first, then the most recently used, then alphabetical.
    rows.sort_by(|(a, used_a), (b, used_b)| {
        b.active
            .cmp(&a.active)
            .then(used_b.cmp(used_a))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });

    Ok(rows.into_iter().map(|(p, _)| p).collect())
}

pub fn save(conn: &Connection, input: &ProductInput) -> Res<Product> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err("نام کالا را بنویسید".to_string());
    }
    let unit = input
        .unit
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .unwrap_or(DEFAULT_UNIT)
        .to_string();

    let same = by_name(conn, &name)?;
    let existing = match input.id {
        Some(id) => {
            let current = load(conn, id)?;
            if let Some(other) = &same {
                if other.id != id {
                    return Err("این نام برای کالای دیگری ثبت شده".to_string());
                }
            }
            Some(current)
        }
        // Typing an old name in a new spelling is the same product, not a second one.
        None => same,
    };

    let now = db::now_ms();
    let (id, action) = match &existing {
        Some(p) => {
            conn.execute(
                "UPDATE product SET name = ?2, unit = ?3, sale_price = ?4, buy_price = ?5,
                        active = ?6, updated_at = ?7
                 WHERE id = ?1",
                rusqlite::params![
                    p.id,
                    name,
                    unit,
                    input.sale_price.unwrap_or(p.sale_price),
                    input.buy_price.unwrap_or(p.buy_price),
                    input.active.unwrap_or(p.active) as i64,
                    now
                ],
            )
            .map_err(|e| fail("ذخیره کالا نشد", e))?;
            (p.id, "update")
        }
        None => {
            conn.execute(
                "INSERT INTO product(name, unit, sale_price, buy_price, active, created_at, updated_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                rusqlite::params![
                    name,
                    unit,
                    input.sale_price.unwrap_or(0),
                    input.buy_price.unwrap_or(0),
                    input.active.unwrap_or(true) as i64,
                    now
                ],
            )
            .map_err(|e| fail("ثبت کالای تازه نشد", e))?;
            (conn.last_insert_rowid(), "create")
        }
    };

    let fresh = load(conn, id)?;

    // A price he typed by hand is a price worth remembering, even when it did
    // not come from an invoice.
    let price_changed = match &existing {
        Some(old) => (old.sale_price, old.buy_price),
        None => (-1, -1),
    };
    if fresh.sale_price > 0 && fresh.sale_price != price_changed.0 {
        remember_price(conn, id, "sale", fresh.sale_price, now)?;
    }
    if fresh.buy_price > 0 && fresh.buy_price != price_changed.1 {
        remember_price(conn, id, "buy", fresh.buy_price, now)?;
    }

    let payload = serde_json::json!({
        "name": fresh.name,
        "unit": fresh.unit,
        "salePrice": fresh.sale_price,
        "buyPrice": fresh.buy_price,
    })
    .to_string();
    db::audit(conn, "product", id, action, &payload)?;

    Ok(fresh)
}

/// One price, written down. The order screen calls this when he types a price
/// that is not on the invoice yet, so «آخرین قیمت» is never a guess.
pub fn remember_price(conn: &Connection, product_id: i64, kind: &str, price: i64, at: i64) -> Res<()> {
    conn.execute(
        "INSERT INTO price_history(product_id, party_id, kind, price, day, at)
         VALUES(?1, NULL, ?2, ?3, ?4, ?5)",
        rusqlite::params![product_id, kind, price, db::today(), at],
    )
    .map_err(|e| fail("ثبت قیمت نشد", e))?;
    Ok(())
}

fn prices<P: rusqlite::Params>(
    conn: &Connection,
    sql: &str,
    args: P,
) -> Res<Vec<PricePoint>> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| fail("سابقه قیمت خوانده نشد", e))?;
    let rows = stmt
        .query_map(args, |r| {
            Ok(PricePoint {
                price: r.get(0)?,
                day: r.get(1)?,
            })
        })
        .map_err(|e| fail("سابقه قیمت خوانده نشد", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| fail("سابقه قیمت خوانده نشد", e))?;
    Ok(rows)
}

/// The prices of one product, newest first. With a `party_id` he gets what he
/// charged *this* customer, which is the number he actually wants to see; if he
/// has never sold it to them, the product's own history is the honest answer.
pub fn history(conn: &Connection, product_id: i64, party_id: Option<i64>) -> Res<Vec<PricePoint>> {
    load(conn, product_id)?;
    let limit = HISTORY_MAX as i64;

    if let Some(party) = party_id {
        let theirs = prices(
            conn,
            "SELECT price, day FROM price_history
             WHERE product_id = ?1 AND party_id = ?2 AND kind = 'sale'
             ORDER BY at DESC, id DESC LIMIT ?3",
            rusqlite::params![product_id, party, limit],
        )?;
        if !theirs.is_empty() {
            return Ok(theirs);
        }
    }

    let sold = prices(
        conn,
        "SELECT price, day FROM price_history
         WHERE product_id = ?1 AND kind = 'sale'
         ORDER BY at DESC, id DESC LIMIT ?2",
        rusqlite::params![product_id, limit],
    )?;
    if !sold.is_empty() {
        return Ok(sold);
    }

    prices(
        conn,
        "SELECT price, day FROM price_history
         WHERE product_id = ?1
         ORDER BY at DESC, id DESC LIMIT ?2",
        rusqlite::params![product_id, limit],
    )
}

/// What this customer actually buys, newest first. This is what the order
/// screen offers before he types a single letter.
pub fn usual(conn: &Connection, party_id: i64) -> Res<Vec<Product>> {
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.name, p.unit, p.sale_price, p.buy_price, p.active,
                    p.created_at, p.updated_at,
                    MAX(d.day) AS last_day, MAX(d.id) AS last_doc
             FROM doc_line l
             JOIN doc d ON d.id = l.doc_id
             JOIN product p ON p.id = l.product_id
             WHERE d.party_id = ?1 AND d.kind = 'sale' AND d.status <> 'void' AND l.voided = 0
             GROUP BY p.id
             ORDER BY last_day DESC, last_doc DESC
             LIMIT ?2",
        )
        .map_err(|e| fail("کالاهای همیشگی خوانده نشد", e))?;
    let rows = stmt
        .query_map(rusqlite::params![party_id, USUAL_MAX as i64], from_row)
        .map_err(|e| fail("کالاهای همیشگی خوانده نشد", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| fail("کالاهای همیشگی خوانده نشد", e))?;
    Ok(rows)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListArgs {
    q: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryArgs {
    product_id: i64,
    party_id: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsualArgs {
    party_id: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveArgs {
    product: ProductInput,
}

pub fn handle(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    Some(match op {
        "products.list" => (|| -> Res<Value> {
            let a: ListArgs = serde_json::from_value(args.clone()).unwrap_or(ListArgs { q: None });
            let conn = ctx.conn()?;
            let list = list(&conn, a.q.as_deref())?;
            serde_json::to_value(list).map_err(|e| fail("فهرست کالاها ساخته نشد", e))
        })(),
        "products.save" => (|| -> Res<Value> {
            let a: SaveArgs = serde_json::from_value(args.clone())
                .map_err(|_| "کالای فرستاده‌شده خوانده نشد".to_string())?;
            let conn = ctx.conn()?;
            let product = save(&conn, &a.product)?;
            serde_json::to_value(product).map_err(|e| fail("کالا ساخته نشد", e))
        })(),
        "products.history" => (|| -> Res<Value> {
            let a: HistoryArgs = serde_json::from_value(args.clone())
                .map_err(|_| "درخواست سابقه قیمت خوانده نشد".to_string())?;
            let conn = ctx.conn()?;
            let points = history(&conn, a.product_id, a.party_id)?;
            serde_json::to_value(points).map_err(|e| fail("سابقه قیمت ساخته نشد", e))
        })(),
        "products.usual" => (|| -> Res<Value> {
            let a: UsualArgs = serde_json::from_value(args.clone())
                .map_err(|_| "درخواست کالاهای همیشگی خوانده نشد".to_string())?;
            let conn = ctx.conn()?;
            let items = usual(&conn, a.party_id)?;
            serde_json::to_value(items).map_err(|e| fail("کالاهای همیشگی ساخته نشد", e))
        })(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::{AppState, Ctx};
    use rusqlite::Connection;
    use std::path::PathBuf;

    /// نخود
    const NOK: &str = "\u{646}\u{62E}\u{648}\u{62F}";
    /// نخود with a ZWNJ typed inside it
    const NOK_ZWNJ: &str = "\u{646}\u{62E}\u{200C}\u{648}\u{62F}";
    /// نخودچی
    const NOK_CHI: &str = "\u{646}\u{62E}\u{648}\u{62F}\u{200C}\u{686}\u{6CC}";
    /// کیک in Persian letters
    const KAK: &str = "\u{6A9}\u{6CC}\u{6A9}";
    /// كيك in Arabic letters
    const KAK_ARABIC: &str = "\u{643}\u{64A}\u{643}";

    fn add_product(conn: &Connection, id: i64, name: &str, unit: &str) {
        conn.execute(
            "INSERT INTO product(id, name, unit, sale_price, buy_price, active, created_at, updated_at)
             VALUES(?1, ?2, ?3, 0, 0, 1, 0, 0)",
            rusqlite::params![id, name, unit],
        )
        .unwrap();
    }

    fn add_party(conn: &Connection, id: i64, name: &str) {
        conn.execute(
            "INSERT INTO party(id, name, kind, created_at, updated_at)
             VALUES(?1, ?2, 'customer', 0, 0)",
            rusqlite::params![id, name],
        )
        .unwrap();
    }

    fn add_sale(conn: &Connection, id: i64, party: i64, day: i64, status: &str) {
        conn.execute(
            "INSERT INTO doc(id, kind, jyear, number, party_id, day, status, total, created_at, updated_at)
             VALUES(?1, 'sale', 1404, ?1, ?2, ?3, ?4, 0, 0, 0)",
            rusqlite::params![id, party, day, status],
        )
        .unwrap();
    }

    fn add_line(conn: &Connection, id: i64, doc: i64, pos: i64, product: i64, voided: bool) {
        conn.execute(
            "INSERT INTO doc_line(id, doc_id, pos, product_id, name, qty_milli, unit, price, total, voided)
             VALUES(?1, ?2, ?3, ?4, 'کالا', 1000, 'کیلو', 100, 100, ?5)",
            rusqlite::params![id, doc, pos, product, voided as i64],
        )
        .unwrap();
    }

    fn ids(list: &[Product]) -> Vec<i64> {
        list.iter().map(|p| p.id).collect()
    }

    /// Test 1: one word typed three ways is one word.
    #[test]
    fn persian_spellings_find_the_same_product() {
        let conn = db::memory();
        add_product(&conn, 1, NOK, DEFAULT_UNIT);
        add_product(&conn, 2, NOK_CHI, DEFAULT_UNIT);
        add_product(&conn, 3, KAK_ARABIC, "عدد");

        // «نخود» finds the plain word and the joined one.
        assert_eq!(ids(&list(&conn, Some(NOK)).unwrap()), vec![1, 2]);
        // The same query with a ZWNJ typed inside it.
        assert_eq!(ids(&list(&conn, Some(NOK_ZWNJ)).unwrap()), vec![1, 2]);
        // «کیک» typed with a Persian kaf and yeh finds the Arabic spelling.
        assert_eq!(ids(&list(&conn, Some(KAK)).unwrap()), vec![3]);
        // And the other way round.
        assert_eq!(ids(&list(&conn, Some(KAK_ARABIC)).unwrap()), vec![3]);

        // Nothing in common, nothing found.
        assert!(list(&conn, Some("برنج")).unwrap().is_empty());
        // No query at all is the whole list.
        assert_eq!(list(&conn, None).unwrap().len(), 3);
    }

    /// Test 2: the order screen offers this customer's own items, nothing else.
    #[test]
    fn usual_is_only_what_this_party_bought() {
        let conn = db::memory();
        add_party(&conn, 1, "لیلا امینی");
        add_party(&conn, 2, "حاج آقا");
        add_product(&conn, 10, NOK, DEFAULT_UNIT);
        add_product(&conn, 11, "عدس", DEFAULT_UNIT);
        add_product(&conn, 12, "برنج", DEFAULT_UNIT);
        add_product(&conn, 13, "زردچوبه", DEFAULT_UNIT);
        add_product(&conn, 14, "سماق", DEFAULT_UNIT);

        // Party 1: نخود ten days ago, عدس two days ago.
        add_sale(&conn, 100, 1, 20_000, "open");
        add_line(&conn, 1000, 100, 1, 10, false);
        add_sale(&conn, 101, 1, 20_002, "open");
        add_line(&conn, 1001, 101, 1, 11, false);
        // A crossed-out line and a cancelled invoice of theirs: neither counts.
        add_line(&conn, 1002, 101, 2, 14, true);
        add_sale(&conn, 102, 1, 20_003, "void");
        add_line(&conn, 1003, 102, 1, 13, false);
        // Party 2 buys something party 1 never did.
        add_sale(&conn, 103, 2, 20_004, "open");
        add_line(&conn, 1004, 103, 1, 12, false);

        // Newest first, and only their own two items.
        assert_eq!(ids(&usual(&conn, 1).unwrap()), vec![11, 10]);
        assert_eq!(ids(&usual(&conn, 2).unwrap()), vec![12]);
        // Someone who has never bought anything gets an empty list, not everybody's.
        assert!(usual(&conn, 3).unwrap().is_empty());
    }

    /// Test 3: the newest price comes first, and there are never more than 40.
    #[test]
    fn history_is_newest_first_and_capped() {
        let conn = db::memory();
        add_product(&conn, 1, NOK, DEFAULT_UNIT);
        for i in 0..45i64 {
            conn.execute(
                "INSERT INTO price_history(product_id, party_id, kind, price, day, at)
                 VALUES(1, NULL, 'sale', ?1, ?2, ?3)",
                rusqlite::params![100 + i, 20_000 + i, 1_000 + i],
            )
            .unwrap();
        }

        let points = history(&conn, 1, None).unwrap();
        assert_eq!(points.len(), 40, "never more than forty prices");
        assert_eq!(points[0].price, 144, "the newest price first");
        assert_eq!(points[39].price, 105);
        assert!(points.windows(2).all(|w| w[0].day > w[1].day), "newest first");

        // An unknown product is a Persian sentence, not an empty list.
        assert_eq!(history(&conn, 999, None).unwrap_err(), "کالا پیدا نشد");
    }

    #[test]
    fn a_known_customer_sees_what_they_were_charged() {
        let conn = db::memory();
        add_party(&conn, 1, "لیلا امینی");
        add_product(&conn, 1, NOK, DEFAULT_UNIT);
        // Two prices for everybody, one of them specifically for leila.
        conn.execute_batch(
            "INSERT INTO price_history(product_id, party_id, kind, price, day, at)
             VALUES(1, NULL, 'sale', 300, 20000, 1000);
             INSERT INTO price_history(product_id, party_id, kind, price, day, at)
             VALUES(1, 1, 'sale', 345, 20001, 2000);",
        )
        .unwrap();

        let theirs = history(&conn, 1, Some(1)).unwrap();
        assert_eq!(theirs.len(), 1);
        assert_eq!(theirs[0].price, 345);
        // A customer with no history of their own falls back to the product's.
        let nobody = history(&conn, 1, Some(2)).unwrap();
        assert_eq!(nobody.len(), 2);
    }

    #[test]
    fn saving_the_same_name_twice_does_not_duplicate_it() {
        let conn = db::memory();
        let first = save(
            &conn,
            &ProductInput {
                name: NOK.to_string(),
                ..ProductInput::default()
            },
        )
        .unwrap();
        assert_eq!(first.unit, DEFAULT_UNIT, "kilogram unless he says otherwise");

        let again = save(
            &conn,
            &ProductInput {
                name: NOK_ZWNJ.to_string(),
                sale_price: Some(345_000),
                ..ProductInput::default()
            },
        )
        .unwrap();
        assert_eq!(again.id, first.id, "the same word is the same product");
        assert_eq!(again.sale_price, 345_000);
        assert_eq!(list(&conn, None).unwrap().len(), 1);

        // The typed price is remembered, so next time it is offered.
        let points = history(&conn, first.id, None).unwrap();
        assert_eq!(points[0].price, 345_000);

        // And the write is in the audit log.
        let audits: i64 = conn
            .query_row("SELECT COUNT(*) FROM audit WHERE entity = 'product'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(audits, 2);
    }

    #[test]
    fn a_product_needs_a_name() {
        let conn = db::memory();
        let err = save(
            &conn,
            &ProductInput {
                name: "   ".to_string(),
                ..ProductInput::default()
            },
        )
        .unwrap_err();
        assert!(err.contains("نام کالا"), "{err}");

        let err = save(
            &conn,
            &ProductInput {
                id: Some(42),
                name: NOK.to_string(),
                ..ProductInput::default()
            },
        )
        .unwrap_err();
        assert_eq!(err, "کالا پیدا نشد");
    }

    fn memory_state() -> AppState {
        AppState {
            db: std::sync::Mutex::new(db::memory()),
            dirs: crate::Dirs {
                data: PathBuf::new(),
                files: PathBuf::new(),
            },
        }
    }

    #[test]
    fn handle_answers_only_its_own_names() {
        let state = memory_state();
        let ctx = Ctx { state: &state };

        assert!(handle(&ctx, "products.list", &serde_json::json!({ "q": null })).is_some());
        assert!(handle(&ctx, "orders.save", &serde_json::json!({})).is_none());

        // The interface sends a product with camelCase names and no unit.
        let saved = handle(
            &ctx,
            "products.save",
            &serde_json::json!({ "product": { "name": NOK, "salePrice": 345_000 } }),
        )
        .unwrap()
        .unwrap();
        let product: Product = serde_json::from_value(saved).unwrap();
        assert_eq!(product.unit, DEFAULT_UNIT);
        assert_eq!(product.sale_price, 345_000);
        assert_eq!(product.buy_price, 0);

        let listed = handle(&ctx, "products.list", &serde_json::json!({ "q": NOK }))
            .unwrap()
            .unwrap();
        let list: Vec<Product> = serde_json::from_value(listed).unwrap();
        assert_eq!(list.len(), 1);
    }
}
