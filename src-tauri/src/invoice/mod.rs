//! Invoice and statement layout: shaped Persian, then a PDF and the SVG preview
//! of exactly the same page.
//!
//! Two things use this module: the interface, which shows the SVG so the
//! shopkeeper sees the page before sending it, and the Telegram bot, which
//! uploads the PDF. They must be the same drawing, so both come from one layout
//! pass and one set of glyph outlines.
//!
//! The shape of the work is: read the document out of the database ([`mod.rs`
//! does that]), lay it out as pages of glyphs and rules ([`layout`]), then write
//! those pages twice — once as SVG for the screen, once as PDF for the paper
//! ([`svg`], [`pdf`]). Persian is shaped by rustybuzz over a small bidi
//! ([`bidi`]) and every letter is drawn as an outline ([`font`]), so nothing in
//! either file depends on a text engine at the other end.

use crate::db::Res;
use crate::domain::{balance, orders, settings};
use crate::Ctx;
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};
use std::path::PathBuf;

mod bidi;
mod font;
mod layout;
mod pdf;
mod svg;
mod words;

use crate::dto::{Doc, DocLine, Party};

/// A page ready to be sent: the PDF file and the SVG of the same page.
pub struct Rendered {
    pub pdf_path: PathBuf,
    pub file_name: String,
    /// The same page as `pdf_path`, for whoever wants to show it rather than
    /// send it. The interface asks for it through `invoice.svg`; the field is
    /// here because a caller that has just built a page should not have to
    /// build it a second time to look at it.
    #[allow(dead_code)]
    pub svg: String,
}

/// The invoice for one document, as a finished file in `dirs.files`.
pub fn render_doc(ctx: &Ctx<'_>, doc_id: i64) -> Res<Rendered> {
    let data = {
        let conn = ctx.conn()?;
        load_invoice(&conn, doc_id)?
    };
    let pages = layout::invoice_pages(&data);
    let file_name = if data.kind == "purchase" {
        format!("purchase-{}-{:04}.pdf", data.jyear, data.number)
    } else {
        format!("invoice-{}-{:04}.pdf", data.jyear, data.number)
    };
    write(ctx, file_name, &pages)
}

/// Every document and payment for a party, with the balance, as one page.
pub fn render_statement(ctx: &Ctx<'_>, party_id: i64) -> Res<Rendered> {
    let data = {
        let conn = ctx.conn()?;
        load_statement(&conn, party_id)?
    };
    let pages = layout::statement_pages(&data);
    let file_name = format!("statement-{party_id}.pdf");
    write(ctx, file_name, &pages)
}

fn write(ctx: &Ctx<'_>, file_name: String, pages: &[layout::Page]) -> Res<Rendered> {
    let pdf = pdf::render(pages);
    let svg = svg::render(pages);
    std::fs::create_dir_all(&ctx.state.dirs.files)
        .map_err(|_| "پوشه فایل ساخته نشد".to_string())?;
    let pdf_path = ctx.state.dirs.files.join(&file_name);
    std::fs::write(&pdf_path, &pdf).map_err(|_| "فایل فاکتور ذخیره نشد".to_string())?;
    Ok(Rendered {
        pdf_path,
        file_name,
        svg,
    })
}

pub fn handle(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    Some(match op {
        "invoice.svg" => (|| {
            let doc_id = arg_id(args)?;
            let data = {
                let conn = ctx.conn()?;
                load_invoice(&conn, doc_id)?
            };
            let pages = layout::invoice_pages(&data);
            Ok(json!({ "svg": svg::render(&pages) }))
        })(),
        "invoice.preview" => (|| {
            let doc_id = arg_id(args)?;
            let rendered = render_doc(ctx, doc_id)?;
            Ok(json!({ "path": rendered.pdf_path.to_string_lossy() }))
        })(),
        _ => return None,
    })
}

fn arg_id(args: &Value) -> Res<i64> {
    args.get("docId")
        .or_else(|| args.get("partyId"))
        .or_else(|| args.get("id"))
        .and_then(Value::as_i64)
        .ok_or_else(|| "شماره فاکتور داده نشد".to_string())
}

// ---------------------------------------------------------------------------
// Reading the document
// ---------------------------------------------------------------------------

fn shop(conn: &Connection) -> layout::Shop {
    let s = settings::load(conn);
    layout::Shop {
        name: s.shop_name,
        phone: s.shop_phone,
        address: s.shop_address,
        note: s.invoice_note,
    }
}

fn load_party(conn: &Connection, party_id: i64) -> Res<Party> {
    conn.query_row(
        "SELECT id, name, phone, kind, address, note, opening, archived, created_at, updated_at
         FROM party WHERE id = ?1",
        [party_id],
        |r| {
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
        },
    )
    .optional()
    .map_err(|e| format!("طرف حساب خوانده نشد: {e}"))?
    .ok_or_else(|| "طرف حساب پیدا نشد".to_string())
}

fn load_invoice(conn: &Connection, doc_id: i64) -> Res<layout::InvoiceData> {
    let sql = format!("SELECT {} FROM doc d WHERE d.id = ?1", orders::DOC_COLUMNS);
    let doc: Doc = conn
        .query_row(&sql, [doc_id], |r| orders::from_row(r))
        .optional()
        .map_err(|e| format!("فاکتور خوانده نشد: {e}"))?
        .ok_or_else(|| "فاکتور پیدا نشد".to_string())?;
    let party = load_party(conn, doc.party_id)?;
    let lines = load_lines(conn, doc_id)?;

    Ok(layout::InvoiceData {
        shop: shop(conn),
        kind: doc.kind,
        jyear: doc.jyear,
        number: doc.number,
        day: doc.day,
        status: doc.status,
        party_name: party.name,
        discount: doc.discount,
        rounding: doc.rounding,
        total: doc.total,
        paid: doc.paid,
        remaining: doc.remaining,
        note: doc.note.unwrap_or_default(),
        lines: lines
            .into_iter()
            .map(|l| layout::LineData {
                pos: l.pos,
                name: l.name,
                qty_milli: l.qty_milli,
                unit: l.unit,
                price: l.price,
                total: l.override_total.unwrap_or(l.total),
                voided: l.voided,
            })
            .collect(),
    })
}

fn load_lines(conn: &Connection, doc_id: i64) -> Res<Vec<DocLine>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, doc_id, pos, product_id, name, qty_milli, unit, price, total,
                    override_total, note, voided
             FROM doc_line WHERE doc_id = ?1 ORDER BY pos, id",
        )
        .map_err(|e| format!("خط‌های فاکتور خوانده نشد: {e}"))?;
    let rows = stmt
        .query_map([doc_id], |r| {
            Ok(DocLine {
                id: r.get(0)?,
                doc_id: r.get(1)?,
                pos: r.get(2)?,
                product_id: r.get(3)?,
                name: r.get(4)?,
                qty_milli: r.get(5)?,
                unit: r.get(6)?,
                price: r.get(7)?,
                total: r.get(8)?,
                override_total: r.get(9)?,
                note: r.get(10)?,
                voided: r.get::<_, i64>(11)? != 0,
            })
        })
        .map_err(|e| format!("خط‌های فاکتور خوانده نشد: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("خط‌های فاکتور خوانده نشد: {e}"))?;
    Ok(rows)
}

fn load_statement(conn: &Connection, party_id: i64) -> Res<layout::StatementData> {
    let party = load_party(conn, party_id)?;
    let balance = balance::of(conn, party_id)?;

    // (day, id, kind) so the two lists can be interleaved newest first, with a
    // document ahead of a payment on the same day.
    let mut items: Vec<(i64, i64, u8, layout::StatementRow)> = Vec::new();

    let sql = format!(
        "SELECT {} FROM doc d WHERE d.party_id = ?1 AND d.status <> 'void'",
        orders::DOC_COLUMNS
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("صورت‌حساب خوانده نشد: {e}"))?;
    let docs = stmt
        .query_map([party_id], |r| orders::from_row(r))
        .map_err(|e| format!("صورت‌حساب خوانده نشد: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("صورت‌حساب خوانده نشد: {e}"))?;
    for doc in docs {
        let title = if doc.kind == "purchase" {
            format!("خرید {}", crate::money::fa_digits(&doc.number.to_string()))
        } else {
            format!("فاکتور فروش {}", crate::money::fa_digits(&doc.number.to_string()))
        };
        items.push((
            doc.day,
            doc.id,
            0,
            layout::StatementRow {
                day: Some(doc.day),
                title,
                amount: doc.total,
                remaining: Some(doc.remaining),
            },
        ));
    }

    let mut stmt = conn
        .prepare(
            "SELECT id, direction, amount, day, method FROM payment
             WHERE party_id = ?1 AND voided = 0",
        )
        .map_err(|e| format!("صورت‌حساب خوانده نشد: {e}"))?;
    let payments = stmt
        .query_map([party_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(|e| format!("صورت‌حساب خوانده نشد: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("صورت‌حساب خوانده نشد: {e}"))?;
    for (id, direction, amount, day, method) in payments {
        let verb = if direction == "out" { "پرداخت" } else { "دریافت" };
        items.push((
            day,
            id,
            1,
            layout::StatementRow {
                day: Some(day),
                title: format!("{verb} {}", method_fa(&method)),
                amount,
                remaining: None,
            },
        ));
    }

    // Nothing older than the first invoice is on the sheet, but an opening
    // balance is part of the answer, so it gets its own line at the bottom.
    if party.opening != 0 {
        items.push((
            i64::MIN,
            0,
            2,
            layout::StatementRow {
                day: None,
                title: "مانده قبلی".to_string(),
                amount: party.opening,
                remaining: None,
            },
        ));
    }

    items.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));

    Ok(layout::StatementData {
        shop: shop(conn),
        party_name: party.name,
        day: crate::db::today(),
        balance,
        rows: items.into_iter().map(|(_, _, _, row)| row).collect(),
    })
}

fn method_fa(method: &str) -> &'static str {
    match method {
        "cash" => "نقد",
        "card" => "کارت",
        "cheque" => "چک",
        _ => "دیگر",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use font::Weight;
    use layout::{Class, Item};
    use std::sync::Mutex;

    /// Everything the tests write lands here so the Lead can open it.
    fn out_dir() -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-out");
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    fn golden_shop(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO setting(key, value) VALUES
                 ('shop_name', 'فروشگاه دایی مراد'),
                 ('shop_phone', '۰۲۱-۵۵۴۴۳۳۲۲'),
                 ('shop_address', 'تهران، بازار آهنگرها، پلاک ۱۲'),
                 ('invoice_note', 'جنس فروخته‌شده تا ۳ روز قابل تعویض است.');",
        )
        .unwrap();
    }

    /// A small shop, a customer, and one invoice with three lines — the page the
    /// Lead should look at.
    fn golden_invoice(conn: &Connection) {
        golden_shop(conn);
        let day = crate::jalali::j2d(1404, 8, 8);
        conn.execute_batch(&format!(
            "INSERT INTO party(id, name, phone, kind, opening, created_at, updated_at)
             VALUES(1, 'لیلا امینی', '۰۹۱۲۳۴۵۶۷۸۹', 'customer', 0, 0, 0);
             INSERT INTO doc(id, kind, jyear, number, party_id, day, status, discount, rounding,
                             total, created_at, updated_at)
             VALUES(1, 'sale', 1404, 12, 1, {day}, 'open', 0, 0, 5175, 0, 0);
             INSERT INTO doc_line(id, doc_id, pos, name, qty_milli, unit, price, total, voided)
             VALUES(1, 1, 1, 'گوجه فرنگی', 15000, 'kg', 345, 5175, 0);
             INSERT INTO doc_line(id, doc_id, pos, name, qty_milli, unit, price, total, voided)
             VALUES(2, 1, 2, 'خیار درجه یک', 3000, 'kg', 210, 630, 0);
             INSERT INTO doc_line(id, doc_id, pos, name, qty_milli, unit, price, total, voided)
             VALUES(3, 1, 3, 'سیب زمینی', 5000, 'kg', 120, 600, 0);
             UPDATE doc SET total = 6405 WHERE id = 1;
             INSERT INTO payment(id, party_id, direction, amount, day, method, created_at)
             VALUES(1, 1, 'in', 2000, {day}, 'cash', 0);
             INSERT INTO allocation(payment_id, doc_id, amount) VALUES(1, 1, 2000);"
        ))
        .unwrap();
    }

    fn baselines(pages: &[layout::Page], class: Class) -> Vec<i32> {
        let mut found: Vec<i32> = pages
            .iter()
            .flat_map(|page| page.items.iter())
            .filter_map(|item| match item {
                Item::Text(text) if text.class == class => {
                    Some((text.baseline * 10.0).round() as i32)
                }
                _ => None,
            })
            .collect();
        found.sort_unstable();
        found.dedup();
        found
    }

    #[test]
    fn shaping_joins_the_letters() {
        // «سلام» is four letters and, once shaped, fewer glyphs: the lam-alef
        // ligature swallows two of them. Each glyph is also a real glyph id for
        // its contextual form, not the code point it came from.
        let shaped = font::typeset("سلام", 10.0, Weight::Regular, bidi::Dir::Rtl);
        assert!(
            shaped.glyphs.len() < "سلام".chars().count(),
            "{} glyphs for four letters",
            shaped.glyphs.len()
        );
        let gids: Vec<u16> = shaped.glyphs.iter().map(|g| g.gid).collect();
        let points: Vec<u16> = "سلام".chars().map(|c| c as u32 as u16).collect();
        for gid in &gids {
            assert!(!points.contains(gid), "{gid} is a code point, not a glyph");
        }
    }

    #[test]
    fn the_number_sits_between_the_words_around_it() {
        let placed = layout::typeset_line("جمع کل ۵٬۱۷۵ تومان", 10.0, Weight::Regular);
        assert_eq!(placed.runs.len(), 3, "{:?}", placed.runs);

        let left_word = placed.runs[0]; // «جمع کل » — read first, so rightmost
        let number = placed.runs[1];
        let right_word = placed.runs[2]; // «تومان» — read last, so leftmost

        assert!(
            right_word.x + right_word.width <= number.x + 0.01,
            "تومان must sit left of the number"
        );
        assert!(
            number.x + number.width <= left_word.x + 0.01,
            "the number must sit left of جمع کل"
        );

        // The same line drawn backwards is a different drawing entirely.
        let page = |text: &str| {
            let mut page = layout::Page::default();
            page.push(layout::line(text, 10.0, Weight::Regular).at(100.0).right(386.0));
            svg::render(&[page])
        };
        let reversed: String = "جمع کل ۵٬۱۷۵ تومان".chars().rev().collect();
        assert_ne!(page("جمع کل ۵٬۱۷۵ تومان"), page(&reversed));
    }

    #[test]
    fn amounts_are_drawn_with_persian_digits() {
        let amount = crate::money::fmt_amount(5_175);
        let persian = font::typeset(&amount, 10.0, Weight::Regular, bidi::Dir::Ltr);
        let latin = font::typeset("5175", 10.0, Weight::Regular, bidi::Dir::Ltr);

        let persian: Vec<u16> = persian.glyphs.iter().map(|g| g.gid).collect();
        let latin: Vec<u16> = latin.glyphs.iter().map(|g| g.gid).collect();
        assert_ne!(persian, latin, "the Persian digits did not reach the shaper");
        assert_eq!(persian.len(), 5);

        // This build of Vazirmatn draws ASCII digits as Persian ones, so the
        // two runs look identical on paper — but they are different glyphs, and
        // the invoice never hands the shaper a Latin digit in the first place.
        for pair in ["0", "5", "9"] {
            let a = font::typeset(&crate::money::fa_digits(pair), 10.0, Weight::Regular, bidi::Dir::Ltr);
            let b = font::typeset(pair, 10.0, Weight::Regular, bidi::Dir::Ltr);
            assert_ne!(a.glyphs[0].gid, b.glyphs[0].gid);
            assert_eq!(
                font::outline(Weight::Regular, a.glyphs[0].gid).len(),
                font::outline(Weight::Regular, b.glyphs[0].gid).len(),
                "the Farsi-Digits build should draw both the same shape"
            );
        }
    }

    #[test]
    fn the_pdf_is_a_real_pdf() {
        let bytes = pdf::render(&[layout::Page::default()]);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("%PDF-1.4"), "no header");
        assert!(text.ends_with("%%EOF\n"), "no trailer");
        assert!(text.contains("/MediaBox [0 0 420 595]"), "no MediaBox");

        // The cross-reference table has to point at the objects it claims. The
        // file carries a binary marker after the header, so the offsets can
        // only be trusted against the raw bytes.
        let start = (0..bytes.len())
            .rev()
            .find(|i| bytes[*i..].starts_with(b"startxref"))
            .expect("no startxref");
        let offset: usize = std::str::from_utf8(&bytes[start + 9..])
            .expect("the trailer is not text")
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .expect("no startxref offset")
            .parse()
            .expect("startxref is not a number");
        assert!(
            bytes[offset..].starts_with(b"xref"),
            "startxref points at {offset}, not at an xref table"
        );

        let table = std::str::from_utf8(&bytes[offset..]).expect("the xref is not text");
        let mut lines = table.lines();
        assert_eq!(lines.next(), Some("xref"));
        let count: usize = lines
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|n| n.parse().ok())
            .expect("no object count");
        assert!(count >= 4, "a page needs four objects");
        let free = lines.next().expect("no free entry");
        assert!(free.starts_with("0000000000 65535 f"), "{free:?}");

        for id in 1..count {
            let row = lines.next().expect("the xref table is short");
            let at: usize = row[..10].parse().expect("an offset is not a number");
            assert!(
                bytes[at..].starts_with(format!("{id} 0 obj").as_bytes()),
                "object {id} is not at {at}"
            );
        }
    }

    #[test]
    fn a_golden_invoice_has_a_baseline_for_every_line() {
        let dir = out_dir();
        let state = crate::AppState {
            db: Mutex::new(db::memory()),
            dirs: crate::Dirs {
                data: dir.clone(),
                files: dir.clone(),
            },
        };
        {
            let conn = state.conn().unwrap();
            golden_invoice(&conn);
        }
        let ctx = Ctx { state: &state };

        let data = {
            let conn = ctx.conn().unwrap();
            load_invoice(&conn, 1).unwrap()
        };
        assert_eq!(data.lines.len(), 3);
        let pages = layout::invoice_pages(&data);
        let rows = baselines(&pages, Class::Row);
        assert_eq!(rows.len(), 3, "three lines, three baselines: {rows:?}");

        let rendered = render_doc(&ctx, 1).unwrap();
        assert_eq!(rendered.file_name, "invoice-1404-0012.pdf");
        assert!(rendered.pdf_path.exists());
        assert!(rendered.svg.starts_with("<svg"));

        // Keep the files where the Lead can open them.
        std::fs::write(dir.join("invoice.pdf"), pdf::render(&pages)).unwrap();
        std::fs::write(dir.join("invoice.svg"), svg::render(&pages)).unwrap();
    }

    #[test]
    fn the_two_operations_the_interface_calls_still_answer() {
        let dir = out_dir();
        let state = crate::AppState {
            db: Mutex::new(db::memory()),
            dirs: crate::Dirs {
                data: dir.clone(),
                files: dir.clone(),
            },
        };
        {
            let conn = state.conn().unwrap();
            golden_invoice(&conn);
        }
        let ctx = Ctx { state: &state };

        let answer = handle(&ctx, "invoice.svg", &json!({ "docId": 1 }))
            .expect("invoice.svg is an invoice operation")
            .expect("the preview renders");
        let svg = answer.get("svg").and_then(Value::as_str).expect("an svg");
        assert!(svg.starts_with("<svg"), "{svg:.20}");
        assert!(svg.ends_with("</svg>"));

        let answer = handle(&ctx, "invoice.preview", &json!({ "docId": 1 }))
            .expect("invoice.preview is an invoice operation")
            .expect("the preview renders");
        let path = answer.get("path").and_then(Value::as_str).expect("a path");
        assert!(path.ends_with("invoice-1404-0012.pdf"), "{path}");
        assert!(std::path::Path::new(path).exists());

        assert!(handle(&ctx, "invoice.nonsense", &json!({})).is_none());
        let err = handle(&ctx, "invoice.svg", &json!({ "docId": 999 }))
            .expect("invoice.svg is an invoice operation")
            .unwrap_err();
        assert_eq!(err, "فاکتور پیدا نشد");
    }

    #[test]
    fn a_purchase_names_who_it_was_bought_from() {
        let dir = out_dir();
        let state = crate::AppState {
            db: Mutex::new(db::memory()),
            dirs: crate::Dirs {
                data: dir.clone(),
                files: dir.clone(),
            },
        };
        {
            let conn = state.conn().unwrap();
            golden_shop(&conn);
            let day = crate::jalali::j2d(1404, 8, 10);
            conn.execute_batch(&format!(
                "INSERT INTO party(id, name, kind, opening, created_at, updated_at)
                 VALUES(3, 'حاج‌آقا رضایی', 'supplier', 0, 0, 0);
                 INSERT INTO doc(id, kind, jyear, number, party_id, day, status, total,
                                 created_at, updated_at)
                 VALUES(7, 'purchase', 1404, 7, 3, {day}, 'open', 1200, 0, 0);
                 INSERT INTO doc_line(id, doc_id, pos, name, qty_milli, unit, price, total, voided)
                 VALUES(7, 7, 1, 'کیسه سیب زمینی', 4000, 'bag', 300, 1200, 0);"
            ))
            .unwrap();
        }
        let ctx = Ctx { state: &state };

        let data = {
            let conn = ctx.conn().unwrap();
            load_invoice(&conn, 7).unwrap()
        };
        assert_eq!(data.kind, "purchase");

        let pages = layout::invoice_pages(&data);
        let titles: Vec<&layout::Text> = pages
            .iter()
            .flat_map(|page| page.items.iter())
            .filter_map(|item| match item {
                Item::Text(text) if text.class == Class::Title => Some(text),
                _ => None,
            })
            .collect();
        assert_eq!(titles.len(), 1);
        assert!(titles[0].glyphs.len() > 8, "«خرید از حاج‌آقا» has letters");

        let rendered = render_doc(&ctx, 7).unwrap();
        assert_eq!(rendered.file_name, "purchase-1404-0007.pdf");
        std::fs::write(dir.join("purchase.pdf"), pdf::render(&pages)).unwrap();
        std::fs::write(dir.join("purchase.svg"), svg::render(&pages)).unwrap();
    }

    #[test]
    fn a_discount_and_a_rounding_are_printed_between_the_sum_and_the_total() {
        let dir = out_dir();
        let state = crate::AppState {
            db: Mutex::new(db::memory()),
            dirs: crate::Dirs {
                data: dir.clone(),
                files: dir.clone(),
            },
        };
        {
            let conn = state.conn().unwrap();
            golden_invoice(&conn);
            // 6,405 less a 175 discount, then rounded up by 25 to 6,255.
            conn.execute_batch("UPDATE doc SET discount = 175, rounding = 25 WHERE id = 1")
                .unwrap();
        }

        let plain = {
            let data = {
                let conn = state.conn().unwrap();
                load_invoice(&conn, 1).unwrap()
            };
            let mut bare = data.clone();
            bare.discount = 0;
            bare.rounding = 0;
            (layout::invoice_pages(&bare), data)
        };
        let (bare_pages, data) = plain;
        let pages = layout::invoice_pages(&data);

        let totals = |set: &[layout::Page]| {
            set.iter()
                .flat_map(|page| page.items.iter())
                .filter(|item| matches!(item, Item::Text(text) if text.class == Class::Total))
                .count()
        };
        // Two extra labelled lines, each with its amount.
        assert_eq!(totals(&pages), totals(&bare_pages) + 4);

        std::fs::write(dir.join("invoice-discount.pdf"), pdf::render(&pages)).unwrap();
        std::fs::write(dir.join("invoice-discount.svg"), svg::render(&pages)).unwrap();
    }

    #[test]
    fn a_cancelled_line_is_struck_through_not_hidden() {
        let dir = out_dir();
        let state = crate::AppState {
            db: Mutex::new(db::memory()),
            dirs: crate::Dirs {
                data: dir.clone(),
                files: dir.clone(),
            },
        };
        {
            let conn = state.conn().unwrap();
            golden_invoice(&conn);
            // The second line is cancelled, and the total follows the sheet.
            conn.execute_batch(
                "UPDATE doc_line SET voided = 1 WHERE id = 2;
                 UPDATE doc SET total = 5775 WHERE id = 1;",
            )
            .unwrap();
        }
        let data = {
            let conn = state.conn().unwrap();
            load_invoice(&conn, 1).unwrap()
        };
        assert_eq!(data.lines.len(), 3);

        let pages = layout::invoice_pages(&data);
        // The cancelled line keeps its row and its place in the numbering.
        assert_eq!(baselines(&pages, Class::Row).len(), 3);
        // A strike is the one thin rule that starts inside the sheet and still
        // reaches the right margin; the totals dividers stop at the label
        // column.
        let strikes = pages
            .iter()
            .flat_map(|page| page.items.iter())
            .filter(|item| {
                matches!(item, Item::Rule(rule)
                    if rule.width == 0.5 && rule.x0 > 100.0 && (rule.x1 - 386.0).abs() < 0.01)
            })
            .count();
        assert_eq!(strikes, 1, "one struck-through line, one strike");

        std::fs::write(dir.join("invoice-voided.pdf"), pdf::render(&pages)).unwrap();
        std::fs::write(dir.join("invoice-voided.svg"), svg::render(&pages)).unwrap();
    }

    #[test]
    fn a_forty_row_statement_paginates_and_keeps_every_row() {
        let dir = out_dir();
        let state = crate::AppState {
            db: Mutex::new(db::memory()),
            dirs: crate::Dirs {
                data: dir.clone(),
                files: dir.clone(),
            },
        };
        {
            let conn = state.conn().unwrap();
            golden_shop(&conn);
            conn.execute_batch(
                "INSERT INTO party(id, name, phone, kind, opening, created_at, updated_at)
                 VALUES(2, 'حاج‌آقا رضایی', '۰۹۱۲۱۱۱۲۲۳۳', 'supplier', 0, 0, 0);",
            )
            .unwrap();
            // Twenty invoices and twenty receipts: forty rows, which is more
            // than one A5 sheet holds.
            let today = crate::db::today();
            for i in 1..=20 {
                let day = today - (20 - i) * 7;
                conn.execute(
                    "INSERT INTO doc(id, kind, jyear, number, party_id, day, status, total,
                                     created_at, updated_at)
                     VALUES(?1, 'sale', 1404, ?1, 2, ?2, 'open', ?3, 0, 0)",
                    rusqlite::params![i, day, 1_000 * i],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO payment(id, party_id, direction, amount, day, method,
                                         created_at)
                     VALUES(?1, 2, 'in', ?2, ?3, 'cash', 0)",
                    rusqlite::params![i, 500 * i, day],
                )
                .unwrap();
            }
        }
        let ctx = Ctx { state: &state };

        let data = {
            let conn = ctx.conn().unwrap();
            load_statement(&conn, 2).unwrap()
        };
        assert_eq!(data.rows.len(), 40, "every document and payment is a row");
        assert!(!data.balance.is_negative());

        let pages = layout::statement_pages(&data);
        assert!(pages.len() > 1, "forty rows do not fit on one sheet");
        let rows = baselines(&pages, Class::Row);
        assert_eq!(rows.len(), 40, "a row went missing over the page break");
        for page in &pages {
            assert!(!page.items.is_empty());
        }

        let rendered = render_statement(&ctx, 2).unwrap();
        assert_eq!(rendered.file_name, "statement-2.pdf");
        std::fs::write(dir.join("statement.pdf"), pdf::render(&pages)).unwrap();
        std::fs::write(dir.join("statement.svg"), svg::render(&pages)).unwrap();
    }

    #[test]
    fn the_thousands_separator_is_the_arabic_one() {
        // At 160 dpi on paper `٬` and `,` are hard to tell apart, so the test
        // asks the shaper which glyph each one actually gets.
        let amount = crate::money::fmt_amount(5_175);
        assert!(amount.contains('\u{066C}'), "fmt_amount gave {amount}");
        assert!(!amount.contains(','), "fmt_amount gave {amount}");

        let shaped = font::typeset(&amount, 9.0, Weight::Regular, bidi::Dir::Ltr);
        let separator = font::typeset("\u{066C}", 9.0, Weight::Regular, bidi::Dir::Ltr);
        let comma = font::typeset(",", 9.0, Weight::Regular, bidi::Dir::Ltr);
        assert_eq!(shaped.glyphs[1].gid, separator.glyphs[0].gid);
        assert_ne!(
            shaped.glyphs[1].gid, comma.glyphs[0].gid,
            "the invoice picked up a Latin comma"
        );
    }

    #[test]
    fn a_document_that_does_not_exist_says_so_in_persian() {
        let dir = out_dir();
        let state = crate::AppState {
            db: Mutex::new(db::memory()),
            dirs: crate::Dirs {
                data: dir.clone(),
                files: dir,
            },
        };
        let ctx = Ctx { state: &state };
        let err = match render_doc(&ctx, 99) {
            Ok(_) => panic!("a document that does not exist must not render"),
            Err(err) => err,
        };
        assert_eq!(err, "فاکتور پیدا نشد");
        assert_eq!(arg_id(&json!({})).unwrap_err(), "شماره فاکتور داده نشد");
    }
}
