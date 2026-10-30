//! The page: what goes where on a sheet of A5.
//!
//! Nothing here knows about PDF or SVG. A page is a list of two things — a run
//! of shaped glyphs and a rule — in points, with the origin at the top left.
//! The two writers turn exactly this into a file, which is why the preview and
//! the thing Telegram sends can never disagree.
//!
//! The picture is deliberately quiet: one weight of ink, a few greys, thin
//! rules instead of boxes, and no colour at all. What a shopkeeper needs from a
//! فاکتور is to find the customer, the line items and the number still owed
//! without reading anything twice, so the eye is led down the right edge (the
//! first that Persian reads) and the totals carry the only rules that are
//! slightly heavier than a hairline.

use super::bidi::Dir;
use super::font::{self, Glyph, Weight};
use crate::jalali;
use crate::money::{fa_digits, fmt_amount, fmt_qty};

pub const PAGE_W: f32 = 420.0;
pub const PAGE_H: f32 = 595.0;

/// The paper's margins: 34 points in from either side, and a footer line that
/// sits low enough to stay out of the way of a full table.
const LEFT: f32 = 34.0;
const RIGHT: f32 = 386.0;
const BOTTOM: f32 = 552.0;
const FOOTER: f32 = 572.0;

/// Ink. Grey rather than black, because a page of pure black rules looks like a
/// form and a page of 92% grey looks like print.
const INK: f32 = 0.09;
const INK_SOFT: f32 = 0.40;
const INK_FAINT: f32 = 0.56;
const RULE: f32 = 0.74;
const RULE_STRONG: f32 = 0.50;

/// Invoice table columns, given as the right edge of each column: a Persian
/// sheet is read right to left, so «ردیف» sits hard against the right margin
/// and «جمع» ends up on the left.
const COL_POS: f32 = 386.0;
const COL_NAME: f32 = 358.0;
const COL_QTY: f32 = 220.0;
const COL_PRICE: f32 = 166.0;
const COL_TOTAL: f32 = 108.0;
const COL_GAP: f32 = 10.0;

/// Statement columns.
const S_DATE: f32 = 386.0;
const S_TITLE: f32 = 316.0;
const S_AMOUNT: f32 = 192.0;
const S_REST: f32 = 112.0;

/// Where the totals sit. They belong on the left of an RTL sheet, and the last
/// number — what is still owed — is the one the customer must not miss.
const TOT_LABEL: f32 = 224.0;
const TOT_AMOUNT: f32 = 148.0;

/// Which part of the page a run belongs to. It is written into the SVG as a
/// data attribute, which makes the file readable by eye and lets the tests ask
/// "how many rows are on this page" without guessing from coordinates.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    Shop,
    Meta,
    Title,
    Party,
    Head,
    Row,
    Total,
    Note,
}

impl Class {
    pub fn name(self) -> &'static str {
        match self {
            Class::Shop => "shop",
            Class::Meta => "meta",
            Class::Title => "title",
            Class::Party => "party",
            Class::Head => "head",
            Class::Row => "row",
            Class::Total => "total",
            Class::Note => "note",
        }
    }
}

/// A run of shaped glyphs placed on the page.
#[derive(Clone, Debug)]
pub struct Text {
    pub x: f32,
    pub baseline: f32,
    pub size: f32,
    pub weight: Weight,
    pub shade: f32,
    pub class: Class,
    pub glyphs: Vec<Glyph>,
    pub width: f32,
}

/// A horizontal rule — a table line, a separator, or a line struck through a
/// cancelled row.
#[derive(Clone, Copy, Debug)]
pub struct Rule {
    pub x0: f32,
    pub x1: f32,
    pub y: f32,
    pub width: f32,
    pub shade: f32,
}

#[derive(Clone, Debug)]
pub enum Item {
    Text(Text),
    Rule(Rule),
}

#[derive(Clone, Debug, Default)]
pub struct Page {
    pub items: Vec<Item>,
}

impl Page {
    pub fn push(&mut self, text: Text) {
        self.items.push(Item::Text(text));
    }

    pub fn rule(&mut self, x0: f32, x1: f32, y: f32, width: f32, shade: f32) {
        self.items.push(Item::Rule(Rule { x0, x1, y, width, shade }));
    }
}

/// Where one directional run landed on a typeset line. The layout itself only
/// needs the glyphs, but the boxes are what the tests use to prove that a
/// number really did land between the two words around it.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub(super) struct RunBox {
    pub x: f32,
    pub width: f32,
}

/// A line after shaping, all in visual order left to right.
pub(super) struct PlacedLine {
    pub glyphs: Vec<Glyph>,
    pub width: f32,
    /// One box per directional run, in the order the line was written.
    #[allow(dead_code)]
    pub runs: Vec<RunBox>,
}

/// Typesets one line of a right-to-left paragraph.
///
/// Each directional run is shaped on its own and the runs are then laid down
/// from the right edge in the order they were written — which is what makes
/// «جمع کل ۵٬۱۷۵ تومان» read as four words and a number instead of a jumble.
/// The glyphs come back in visual order, so a writer only ever has to move
/// leftwards to right.
pub(super) fn typeset_line(text: &str, size: f32, weight: Weight) -> PlacedLine {
    let runs = super::bidi::runs(text, Dir::PARAGRAPH);
    let parts: Vec<font::Typeset> = runs
        .iter()
        .map(|run| font::typeset(run.text, size, weight, run.dir))
        .collect();
    let width: f32 = parts.iter().map(|part| part.width).sum();

    let mut boxes = Vec::with_capacity(parts.len());
    let mut glyphs = Vec::new();
    let mut pen = width;
    for part in &parts {
        pen -= part.width;
        boxes.push(RunBox {
            x: pen,
            width: part.width,
        });
        for glyph in &part.glyphs {
            glyphs.push(Glyph {
                gid: glyph.gid,
                dx: pen + glyph.dx,
                dy: glyph.dy,
                adv: glyph.adv,
            });
        }
    }
    glyphs.sort_by(|a, b| a.dx.partial_cmp(&b.dx).unwrap_or(std::cmp::Ordering::Equal));

    PlacedLine {
        glyphs,
        width,
        runs: boxes,
    }
}

/// Starts a line of Persian at no particular place; the builder methods below
/// put it where the sheet wants it.
pub(super) fn line(text: &str, size: f32, weight: Weight) -> Text {
    let placed = typeset_line(text, size, weight);
    Text {
        x: 0.0,
        baseline: 0.0,
        size,
        weight,
        shade: INK,
        class: Class::Row,
        glyphs: placed.glyphs,
        width: placed.width,
    }
}

impl Text {
    pub(super) fn shade(mut self, shade: f32) -> Text {
        self.shade = shade;
        self
    }

    pub(super) fn class(mut self, class: Class) -> Text {
        self.class = class;
        self
    }

    /// Puts the run against a right edge — the usual case on this sheet.
    pub(super) fn right(mut self, x: f32) -> Text {
        self.x = x - self.width;
        self
    }

    pub(super) fn left(mut self, x: f32) -> Text {
        self.x = x;
        self
    }

    pub(super) fn at(mut self, baseline: f32) -> Text {
        self.baseline = baseline;
        self
    }
}

/// A number the way both writers spell it: no exponent, no trailing zeros, and
/// never `-0`. Two decimals is finer than a sheet of A5 can show.
pub fn num(v: f32) -> String {
    let v = if v.abs() < 0.005 { 0.0 } else { v };
    let mut s = format!("{v:.2}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s.is_empty() || s == "-" {
        s = "0".to_string();
    }
    s
}

/// A matrix coefficient — the multiplier that takes font units to points. It is
/// a small number (a nine point glyph is 0.0044 of its outline) and rounding it
/// to two decimals would round it to nothing at all, which draws nothing, so it
/// gets a writer of its own.
pub fn num_fine(v: f32) -> String {
    let mut s = format!("{v:.6}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s.is_empty() || s == "-" {
        s = "0".to_string();
    }
    s
}

/// The width a string will take, in points.
pub fn measure(text: &str, size: f32, weight: Weight) -> f32 {
    typeset_line(text, size, weight).width
}

/// Trims a string to fit a column, with an ellipsis. Descriptions and notes are
/// typed by hand and can be longer than the space they get; a line that runs
/// into the next column is worse than a line that ends in «…».
pub fn fit(text: &str, size: f32, weight: Weight, max: f32) -> String {
    if measure(text, size, weight) <= max {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut cut = chars.len();
    while cut > 0 {
        let head: String = chars[..cut].iter().collect::<String>().trim_end().to_string();
        let candidate = format!("{head}…");
        if measure(&candidate, size, weight) <= max {
            return candidate;
        }
        cut -= 1;
    }
    String::new()
}

/// Persian names for the unit codes a shopkeeper might have picked.
fn unit_fa(unit: &str) -> String {
    match unit.trim() {
        "" => String::new(),
        "kg" => "کیلو".to_string(),
        "g" => "گرم".to_string(),
        "piece" | "pcs" => "عدد".to_string(),
        "pack" => "بسته".to_string(),
        "bag" => "کیسه".to_string(),
        "box" => "جعبه".to_string(),
        "jar" => "شیشه".to_string(),
        "litre" | "liter" | "l" => "لیتر".to_string(),
        "metre" | "meter" | "m" => "متر".to_string(),
        "tonne" | "ton" => "تن".to_string(),
        other => other.to_string(),
    }
}

/// The shop as it appears at the top of every page.
#[derive(Clone, Debug, Default)]
pub struct Shop {
    pub name: String,
    pub phone: String,
    pub address: String,
    pub note: String,
}

/// A page under construction, with the pen moving down it.
struct Sheet {
    pages: Vec<Page>,
    page: Page,
    y: f32,
}

impl Sheet {
    fn new() -> Sheet {
        Sheet {
            pages: Vec::new(),
            page: Page::default(),
            y: 0.0,
        }
    }

    fn has_room(&self, need: f32) -> bool {
        self.y + need <= BOTTOM
    }

    fn page_break(&mut self) {
        self.y = 0.0;
        self.pages.push(std::mem::take(&mut self.page));
    }

    fn finish(mut self) -> Vec<Page> {
        if !self.page.items.is_empty() {
            self.pages.push(self.page);
        }
        if self.pages.is_empty() {
            self.pages.push(Page::default());
        }
        self.pages
    }
}

// ---------------------------------------------------------------------------
// The masthead, shared by the invoice and the statement
// ---------------------------------------------------------------------------

fn masthead(sheet: &mut Sheet, shop: &Shop) {
    let name = shop.name.trim();
    if !name.is_empty() {
        let width = RIGHT - LEFT - 150.0;
        sheet.page.push(
            line(&fit(name, 15.0, Weight::Bold, width), 15.0, Weight::Bold)
                .class(Class::Shop)
                .at(46.0)
                .right(RIGHT),
        );
    }

    let contact = [
        shop.phone.trim().to_string(),
        shop.address.trim().to_string(),
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join(" · ");
    if !contact.is_empty() {
        sheet.page.push(
            line(&fit(&contact, 7.8, Weight::Regular, 300.0), 7.8, Weight::Regular)
                .class(Class::Meta)
                .shade(INK_SOFT)
                .at(60.0)
                .right(RIGHT),
        );
    }

    sheet.page.rule(LEFT, RIGHT, 70.0, 0.9, RULE_STRONG);
    sheet.y = 92.0;
}

// ---------------------------------------------------------------------------
// The invoice
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct LineData {
    pub pos: i64,
    pub name: String,
    pub qty_milli: i64,
    pub unit: String,
    pub price: i64,
    pub total: i64,
    pub voided: bool,
}

#[derive(Clone, Debug)]
pub struct InvoiceData {
    pub shop: Shop,
    /// `sale` or `purchase`.
    pub kind: String,
    pub jyear: i64,
    pub number: i64,
    pub day: i64,
    pub status: String,
    pub party_name: String,
    pub discount: i64,
    pub rounding: i64,
    pub total: i64,
    pub paid: i64,
    pub remaining: i64,
    pub note: String,
    pub lines: Vec<LineData>,
}

impl InvoiceData {
    fn is_sale(&self) -> bool {
        self.kind != "purchase"
    }
}

/// The invoice for one document, as one or more finished pages.
pub fn invoice_pages(data: &InvoiceData) -> Vec<Page> {
    let mut sheet = Sheet::new();
    let sale = data.is_sale();
    masthead(&mut sheet, &data.shop);

    // The title, with the document number tucked in beside it.
    let title = if sale {
        "فاکتور فروش".to_string()
    } else {
        format!("خرید از {}", data.party_name.trim())
    };
    let title = fit(&title, 13.5, Weight::Bold, 158.0);
    let title_width = measure(&title, 13.5, Weight::Bold);
    sheet
        .page
        .push(line(&title, 13.5, Weight::Bold).class(Class::Title).at(96.0).right(RIGHT));
    let number = format!("شماره {}", fa_digits(&data.number.to_string()));
    sheet.page.push(
        line(&number, 9.0, Weight::Regular)
            .class(Class::Meta)
            .shade(INK_SOFT)
            .at(96.0)
            .right(RIGHT - title_width - 9.0),
    );

    // The date belongs where the eye expects it on a Persian invoice: top left.
    sheet.page.push(
        line(&jalali::long(data.day), 9.0, Weight::Regular)
            .class(Class::Meta)
            .shade(INK_SOFT)
            .at(96.0)
            .right(200.0),
    );

    if sale {
        let who = format!("خریدار: {}", data.party_name.trim());
        sheet.page.push(
            line(&fit(&who, 10.5, Weight::Regular, 260.0), 10.5, Weight::Regular)
                .class(Class::Party)
                .at(118.0)
                .right(RIGHT),
        );
    }

    // A cancelled document must never look like a live one, even if someone
    // opens the file from the folder months later.
    if data.status == "void" {
        sheet.page.push(
            line("باطل شده", 9.5, Weight::Medium)
                .class(Class::Meta)
                .shade(0.35)
                .at(118.0)
                .right(200.0),
        );
    }
    sheet.page.rule(LEFT, RIGHT, 129.0, 0.4, 0.87);

    invoice_table(&mut sheet, data, sale);
    invoice_totals(&mut sheet, data, sale);
    invoice_note(&mut sheet, data);

    let mut pages = sheet.finish();
    stamp_footer(&mut pages, &data.shop.note);
    pages
}

fn invoice_head(page: &mut Page, baseline: f32) {
    for (label, x) in [
        ("ردیف", COL_POS),
        ("شرح کالا", COL_NAME),
        ("مقدار", COL_QTY),
        ("فی", COL_PRICE),
        ("جمع", COL_TOTAL),
    ] {
        page.push(
            line(label, 8.0, Weight::Medium)
                .class(Class::Head)
                .shade(0.30)
                .at(baseline)
                .right(x),
        );
    }
}

/// What a continued invoice looks like: the shop, the number, and the columns
/// again, so page two can be read on its own.
fn invoice_continuation(sheet: &mut Sheet, data: &InvoiceData, sale: bool) {
    let name = data.shop.name.trim();
    if !name.is_empty() {
        sheet.page.push(
            line(&fit(name, 10.5, Weight::Bold, 240.0), 10.5, Weight::Bold)
                .class(Class::Shop)
                .shade(INK_SOFT)
                .at(42.0)
                .right(RIGHT),
        );
    }
    let title = if sale { "فاکتور فروش" } else { "خرید" };
    let label = format!("{title} {} — ادامه", fa_digits(&data.number.to_string()));
    sheet.page.push(
        line(&label, 8.5, Weight::Regular)
            .class(Class::Meta)
            .shade(INK_SOFT)
            .at(54.0)
            .right(RIGHT),
    );
    sheet.page.rule(LEFT, RIGHT, 62.0, 0.6, RULE);
    invoice_head(&mut sheet.page, 78.0);
    sheet.page.rule(LEFT, RIGHT, 84.0, 0.8, RULE_STRONG);
    sheet.y = 99.0;
}

const ROW_STEP: f32 = 17.5;

fn invoice_table(sheet: &mut Sheet, data: &InvoiceData, sale: bool) {
    invoice_head(&mut sheet.page, 146.0);
    sheet.page.rule(LEFT, RIGHT, 152.0, 0.8, RULE_STRONG);
    sheet.y = 167.0;

    if data.lines.is_empty() {
        sheet.page.push(
            line("خطی ثبت نشده است", 9.0, Weight::Regular)
                .class(Class::Row)
                .shade(INK_FAINT)
                .at(sheet.y)
                .right(COL_NAME),
        );
        sheet.y += ROW_STEP;
    }

    for (i, item) in data.lines.iter().enumerate() {
        if !sheet.has_room(ROW_STEP + 30.0) {
            sheet.page_break();
            invoice_continuation(sheet, data, sale);
        }
        let y = sheet.y;
        let ink = if item.voided { 0.46 } else { INK };

        let index = if item.pos > 0 {
            item.pos
        } else {
            i as i64 + 1
        };
        let pen = if item.voided { 0.46 } else { INK_FAINT };
        sheet
            .page
            .push(line(&fa_digits(&index.to_string()), 8.5, Weight::Regular)
                .class(Class::Row)
                .shade(pen)
                .at(y)
                .right(COL_POS));

        let name = fit(&item.name, 9.0, Weight::Regular, COL_NAME - COL_QTY - COL_GAP);
        sheet.page.push(
            line(&name, 9.0, Weight::Regular)
                .class(Class::Row)
                .shade(ink)
                .at(y)
                .right(COL_NAME),
        );

        let unit = unit_fa(&item.unit);
        let qty = if unit.is_empty() {
            fmt_qty(item.qty_milli)
        } else {
            format!("{} {}", fmt_qty(item.qty_milli), unit)
        };
        let qty = fit(&qty, 8.5, Weight::Regular, COL_QTY - COL_PRICE - COL_GAP);
        sheet.page.push(
            line(&qty, 8.5, Weight::Regular)
                .class(Class::Row)
                .shade(ink)
                .at(y)
                .right(COL_QTY),
        );

        let price = fit(
            &fmt_amount(item.price),
            8.5,
            Weight::Regular,
            COL_PRICE - COL_TOTAL - COL_GAP,
        );
        sheet.page.push(
            line(&price, 8.5, Weight::Regular)
                .class(Class::Row)
                .shade(ink)
                .at(y)
                .right(COL_PRICE),
        );

        // A cancelled line keeps its place on the sheet and says so, rather
        // than quietly vanishing and leaving a gap in the numbering.
        let total = if item.voided {
            "خط خورده".to_string()
        } else {
            fmt_amount(item.total)
        };
        let total = fit(&total, 8.5, Weight::Regular, COL_TOTAL - LEFT);
        sheet.page.push(
            line(&total, 8.5, Weight::Regular)
                .class(Class::Row)
                .shade(ink)
                .at(y)
                .right(COL_TOTAL),
        );

        if item.voided {
            // Struck from the row number to the price, so the words «خط خورده»
            // in the last column stay readable.
            sheet.page.rule(COL_TOTAL, RIGHT, y - 3.0, 0.5, 0.46);
        }
        // The last row's line is the foot of the table, so it is drawn as an
        // edge rather than as another hairline.
        let foot = i + 1 == data.lines.len();
        sheet.page.rule(
            LEFT,
            RIGHT,
            y + 7.0,
            if foot { 0.8 } else { 0.3 },
            if foot { RULE_STRONG } else { 0.90 },
        );
        sheet.y += ROW_STEP;
    }
}

/// One line of the totals block: the label against the block's right edge and
/// the amount against the amount edge, so every figure on the sheet lines up.
fn tot_line(
    page: &mut Page,
    label: &str,
    amount: &str,
    y: f32,
    size: f32,
    weight: Weight,
    shade: f32,
) {
    page.push(
        line(label, size, weight)
            .class(Class::Total)
            .shade(shade)
            .at(y)
            .right(TOT_LABEL),
    );
    if !amount.is_empty() {
        page.push(
            line(amount, size, weight)
                .class(Class::Total)
                .shade(shade)
                .at(y)
                .right(TOT_AMOUNT),
        );
    }
}

fn invoice_totals(sheet: &mut Sheet, data: &InvoiceData, sale: bool) {
    if !sheet.has_room(150.0) {
        sheet.page_break();
        invoice_continuation(sheet, data, sale);
    }

    // The block is one column: every rule in it runs from the left margin to
    // just past the labels, so the whole thing reads as one sum instead of
    // three unrelated fragments.
    let base = sheet.y + 12.0;
    sheet.page.rule(LEFT, TOT_LABEL, base, 0.8, RULE_STRONG);

    // The lines add up in public: what the goods came to, then what was taken
    // off, then what the total was rounded by.
    let gross = data.total + data.discount - data.rounding;
    let mut y = base + 18.0;
    let mut last = y;

    tot_line(
        &mut sheet.page,
        "جمع کل",
        &fmt_amount(gross),
        y,
        9.5,
        Weight::Regular,
        INK,
    );
    if data.discount != 0 {
        y += 13.5;
        tot_line(
            &mut sheet.page,
            "تخفیف",
            &fmt_amount(-data.discount),
            y,
            8.8,
            Weight::Regular,
            INK_SOFT,
        );
        last = y;
    }
    if data.rounding != 0 {
        y += 13.5;
        tot_line(
            &mut sheet.page,
            "گرد کردن",
            &fmt_amount(data.rounding),
            y,
            8.8,
            Weight::Regular,
            INK_SOFT,
        );
        last = y;
    }

    // Air, a divider, air: a new group — the total and what has been paid
    // against it.
    y = last + 12.0;
    sheet.page.rule(LEFT, TOT_LABEL, y, 0.5, RULE_STRONG);
    y += 20.0;
    tot_line(
        &mut sheet.page,
        "مبلغ نهایی",
        &fmt_amount(data.total),
        y,
        11.5,
        Weight::Bold,
        INK,
    );
    last = y;
    if data.paid > 0 {
        y += 15.0;
        tot_line(
            &mut sheet.page,
            "پرداخت شده",
            &fmt_amount(data.paid),
            y,
            9.0,
            Weight::Regular,
            INK_SOFT,
        );
        last = y;
    }

    if data.remaining == 0 && data.paid == 0 {
        sheet.y = last + 6.0;
        return;
    }

    // The one number the customer must not miss, after its own break.
    y = last + 12.0;
    sheet.page.rule(LEFT, TOT_LABEL, y, 0.5, RULE_STRONG);
    y += 22.0;
    if data.remaining != 0 {
        tot_line(
            &mut sheet.page,
            "مانده",
            &fmt_amount(data.remaining),
            y,
            14.0,
            Weight::Bold,
            INK,
        );
    } else {
        tot_line(
            &mut sheet.page,
            "تسویه شد",
            "",
            y,
            11.0,
            Weight::Medium,
            INK_SOFT,
        );
    }
    sheet.y = y + 4.0;
}

fn invoice_note(sheet: &mut Sheet, data: &InvoiceData) {
    let note = data.note.trim();
    if note.is_empty() {
        return;
    }
    if !sheet.has_room(26.0) {
        sheet.page_break();
    }
    let y = sheet.y + 16.0;
    sheet.page.push(
        line(&fit(note, 8.2, Weight::Regular, RIGHT - LEFT), 8.2, Weight::Regular)
            .class(Class::Note)
            .shade(INK_SOFT)
            .at(y)
            .right(RIGHT),
    );
    sheet.y = y;
}

// ---------------------------------------------------------------------------
// The statement
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct StatementRow {
    pub day: Option<i64>,
    pub title: String,
    pub amount: i64,
    pub remaining: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct StatementData {
    pub shop: Shop,
    pub party_name: String,
    /// Today, in Jalali, printed at the top of the sheet.
    pub day: i64,
    /// Positive: the party owes the shop.
    pub balance: i64,
    /// Newest first.
    pub rows: Vec<StatementRow>,
}

pub fn statement_pages(data: &StatementData) -> Vec<Page> {
    let mut sheet = Sheet::new();
    masthead(&mut sheet, &data.shop);

    sheet.page.push(
        line("صورت‌حساب", 13.5, Weight::Bold)
            .class(Class::Title)
            .at(96.0)
            .right(RIGHT),
    );
    sheet.page.push(
        line(&jalali::long(data.day), 9.0, Weight::Regular)
            .class(Class::Meta)
            .shade(INK_SOFT)
            .at(96.0)
            .right(200.0),
    );
    let name = fit(data.party_name.trim(), 11.0, Weight::Regular, 260.0);
    sheet.page.push(
        line(&name, 11.0, Weight::Regular)
            .class(Class::Party)
            .at(118.0)
            .right(RIGHT),
    );
    sheet.page.rule(LEFT, RIGHT, 129.0, 0.4, 0.87);

    statement_head(&mut sheet.page, 146.0);
    sheet.page.rule(LEFT, RIGHT, 152.0, 0.8, RULE_STRONG);
    sheet.y = 167.0;

    const STEP: f32 = 15.5;

    if data.rows.is_empty() {
        sheet.page.push(
            line("حرکتی ثبت نشده است", 9.0, Weight::Regular)
                .class(Class::Row)
                .shade(INK_FAINT)
                .at(sheet.y)
                .right(S_TITLE),
        );
        sheet.y += STEP;
    }

    for item in &data.rows {
        if !sheet.has_room(STEP + 34.0) {
            sheet.page_break();
            statement_continuation(&mut sheet, data, "ادامه");
        }
        let y = sheet.y;

        let date = item.day.map(jalali::short).unwrap_or_default();
        sheet.page.push(
            line(&date, 8.4, Weight::Regular)
                .class(Class::Row)
                .shade(INK_FAINT)
                .at(y)
                .right(S_DATE),
        );
        let title = fit(&item.title, 8.8, Weight::Regular, S_TITLE - S_AMOUNT - COL_GAP);
        sheet.page.push(
            line(&title, 8.8, Weight::Regular)
                .class(Class::Row)
                .shade(INK)
                .at(y)
                .right(S_TITLE),
        );
        let amount = fmt_amount(item.amount);
        sheet.page.push(
            line(&amount, 8.8, Weight::Regular)
                .class(Class::Row)
                .shade(INK)
                .at(y)
                .right(S_AMOUNT),
        );
        if let Some(rest) = item.remaining {
            let rest = fmt_amount(rest);
            sheet.page.push(
                line(&rest, 8.8, Weight::Regular)
                    .class(Class::Row)
                    .shade(INK_SOFT)
                    .at(y)
                    .right(S_REST),
            );
        }

        sheet.page.rule(LEFT, RIGHT, y + 6.0, 0.3, 0.90);
        sheet.y += STEP;
    }

    statement_balance(&mut sheet, data);
    let mut pages = sheet.finish();
    stamp_footer(&mut pages, &data.shop.note);
    pages
}

fn statement_head(page: &mut Page, baseline: f32) {
    for (label, x) in [
        ("تاریخ", S_DATE),
        ("شرح", S_TITLE),
        ("مبلغ", S_AMOUNT),
        ("مانده", S_REST),
    ] {
        page.push(
            line(label, 8.0, Weight::Medium)
                .class(Class::Head)
                .shade(0.30)
                .at(baseline)
                .right(x),
        );
    }
}

fn statement_continuation(sheet: &mut Sheet, data: &StatementData, what: &str) {
    let name = data.shop.name.trim();
    if !name.is_empty() {
        sheet.page.push(
            line(&fit(name, 10.5, Weight::Bold, 240.0), 10.5, Weight::Bold)
                .class(Class::Shop)
                .shade(INK_SOFT)
                .at(42.0)
                .right(RIGHT),
        );
    }
    sheet.page.push(
        line(&format!("صورت‌حساب {} — {what}", data.party_name.trim()), 8.5, Weight::Regular)
            .class(Class::Meta)
            .shade(INK_SOFT)
            .at(54.0)
            .right(RIGHT),
    );
    sheet.page.rule(LEFT, RIGHT, 62.0, 0.6, RULE);
    statement_head(&mut sheet.page, 78.0);
    sheet.page.rule(LEFT, RIGHT, 84.0, 0.8, RULE_STRONG);
    sheet.y = 99.0;
}

fn statement_balance(sheet: &mut Sheet, data: &StatementData) {
    if !sheet.has_room(78.0) {
        sheet.page_break();
        statement_continuation(sheet, data, "مانده");
    }
    let base = sheet.y + 12.0;
    sheet.page.rule(LEFT, RIGHT, base, 0.9, RULE_STRONG);

    let amount = format!("{} تومان", fmt_amount(data.balance.abs()));
    let words = format!("مانده حساب: {amount}");
    sheet.page.push(
        line(&words, 12.5, Weight::Bold)
            .class(Class::Total)
            .at(base + 24.0)
            .right(RIGHT),
    );

    let (side, shade) = match data.balance {
        b if b > 0 => ("طلب شما از ایشان", INK),
        b if b < 0 => ("بدهی شما به ایشان", INK),
        _ => ("حساب تسویه است", INK_SOFT),
    };
    sheet.page.push(
        line(side, 9.0, Weight::Regular)
            .class(Class::Total)
            .shade(shade)
            .at(base + 40.0)
            .right(RIGHT),
    );
    sheet.y = base + 40.0;
}

// ---------------------------------------------------------------------------
// The footer
// ---------------------------------------------------------------------------

fn stamp_footer(pages: &mut [Page], note: &str) {
    let count = pages.len();
    let note = note.trim();
    for (i, page) in pages.iter_mut().enumerate() {
        if !note.is_empty() {
            page.push(
                line(&fit(note, 7.6, Weight::Regular, 250.0), 7.6, Weight::Regular)
                    .class(Class::Note)
                    .shade(0.50)
                    .at(FOOTER)
                    .right(RIGHT),
            );
        }
        if count > 1 {
            let label = format!(
                "صفحه {} از {}",
                fa_digits(&(i + 1).to_string()),
                fa_digits(&count.to_string())
            );
            page.push(
                line(&label, 7.6, Weight::Regular)
                    .class(Class::Note)
                    .shade(0.55)
                    .at(FOOTER)
                    .left(LEFT),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_description_is_trimmed_not_overflowed() {
        let long = "گوجه فرنگی گلخانه‌ای درجه یک بسیار تازه و مرغوب از روستای دماوند";
        let fitted = fit(long, 9.0, Weight::Regular, 120.0);
        assert!(fitted.ends_with('…'), "{fitted}");
        assert!(measure(&fitted, 9.0, Weight::Regular) <= 120.0);
    }

    #[test]
    fn units_come_out_in_persian() {
        assert_eq!(unit_fa("kg"), "کیلو");
        assert_eq!(unit_fa("piece"), "عدد");
        assert_eq!(unit_fa(""), "");
        assert_eq!(unit_fa("کیلو"), "کیلو");
    }

    #[test]
    fn the_totals_never_cross_the_right_edge_of_the_left_block() {
        // The biggest number on the sheet still has to fit between the margin
        // and the amount column.
        let amount = fmt_amount(1_234_567);
        let width = measure(&amount, 14.0, Weight::Bold);
        assert!(TOT_AMOUNT - width >= LEFT, "{width}");
        let label = measure("مبلغ نهایی", 11.5, Weight::Bold);
        assert!(TOT_LABEL - label >= TOT_AMOUNT, "the label runs into the amount");
    }

    #[test]
    fn the_last_line_of_a_statement_follows_the_sign_of_the_balance() {
        let data = |balance: i64| StatementData {
            shop: Shop {
                name: "فروشگاه".to_string(),
                ..Shop::default()
            },
            party_name: "لیلا امینی".to_string(),
            day: crate::jalali::j2d(1404, 8, 8),
            balance,
            rows: vec![StatementRow {
                day: Some(crate::jalali::j2d(1404, 8, 8)),
                title: "فاکتور فروش ۱۲".to_string(),
                amount: 5_175,
                remaining: Some(5_175),
            }],
        };
        let draw = |balance| {
            let pages = statement_pages(&data(balance));
            super::super::svg::render(&pages)
        };
        // «طلب شما از ایشان» and «بدهی شما به ایشان» are different words, so
        // the two sheets cannot be the same drawing.
        assert_ne!(draw(5_175), draw(-5_175));
        assert_ne!(draw(5_175), draw(0));
        assert!(draw(0).contains("</svg>"));
    }
}
