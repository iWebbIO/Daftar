//! The page as SVG — the picture the app shows before anything is sent.
//!
//! Every letter is a `<path>` of the same glyph outline the PDF fills, put in
//! place by the same numbers, so the preview cannot drift from the paper. Text
//! is not text here on purpose: there is no font to load and no shaping for the
//! viewer to get wrong.

use super::font::{self, Seg};
use super::layout::{frame_bars, num, num_fine, Item, Page, PAGE_H, PAGE_W};

/// The air between one page and the next when a preview shows several.
const GAP: f32 = 22.0;

pub fn render(pages: &[Page]) -> String {
    let count = pages.len().max(1) as f32;
    let height = count * PAGE_H + (count - 1.0) * GAP;

    let mut out = String::with_capacity(48 * 1024);
    out.push_str(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#,
        w = num(PAGE_W),
        h = num(height),
    ));

    for (i, page) in pages.iter().enumerate() {
        out.push_str(&format!(
            r#"<g transform="translate(0 {})">"#,
            num(i as f32 * (PAGE_H + GAP))
        ));
        out.push_str(&format!(
            r##"<rect width="{}" height="{}" fill="#ffffff" stroke="#ececec" stroke-width="0.5"/>"##,
            num(PAGE_W),
            num(PAGE_H)
        ));
        for item in &page.items {
            match item {
                Item::Rule(rule) => out.push_str(&bar(
                    rule.x0,
                    rule.y - rule.width / 2.0,
                    rule.x1,
                    rule.y + rule.width / 2.0,
                    rule.shade,
                )),
                Item::Frame(frame) => {
                    for (x0, y0, x1, y1) in frame_bars(frame) {
                        out.push_str(&bar(x0, y0, x1, y1, frame.shade));
                    }
                }
                Item::Text(text) => {
                    if text.glyphs.is_empty() {
                        continue;
                    }
                    let scale = font::scale(text.weight, text.size);
                    if scale <= 0.0 {
                        continue;
                    }
                    out.push_str(&format!(
                        r#"<g fill="{}" data-class="{}">"#,
                        hex(text.shade),
                        text.class.name()
                    ));
                    for glyph in &text.glyphs {
                        let segs = font::outline(text.weight, glyph.gid);
                        if segs.is_empty() {
                            continue;
                        }
                        out.push_str(&format!(
                            r#"<path transform="matrix({s} 0 0 -{s} {x} {y})" d="{}"/>"#,
                            path_data(&segs),
                            s = num_fine(scale),
                            x = num(text.x + glyph.dx),
                            y = num(text.baseline - glyph.dy),
                        ));
                    }
                    out.push_str("</g>");
                }
            }
        }
        out.push_str("</g>");
    }

    out.push_str("</svg>");
    out
}

/// The outline in font units, in SVG's own path language. The matrix on the
/// `<path>` scales it and flips y, so the numbers here stay the integers the
/// font was drawn with.
pub fn path_data(segs: &[Seg]) -> String {
    let mut out = String::with_capacity(segs.len() * 12);
    for seg in segs {
        let piece = match seg {
            Seg::Move(x, y) => format!("M{} {}", num(*x), num(*y)),
            Seg::Line(x, y) => format!("L{} {}", num(*x), num(*y)),
            Seg::Quad(cx, cy, x, y) => {
                format!("Q{} {} {} {}", num(*cx), num(*cy), num(*x), num(*y))
            }
            Seg::Cubic(x1, y1, x2, y2, x, y) => format!(
                "C{} {} {} {} {} {}",
                num(*x1),
                num(*y1),
                num(*x2),
                num(*y2),
                num(*x),
                num(*y)
            ),
            Seg::Close => "Z".to_string(),
        };
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&piece);
    }
    out
}

/// A grey level as a colour. The sheet has no colour of its own.
pub fn hex(shade: f32) -> String {
    let v = (shade.clamp(0.0, 1.0) * 255.0).round() as u32;
    format!("#{v:02x}{v:02x}{v:02x}")
}

/// One filled rectangle — what both a rule and a bar of a frame are made of.
fn bar(x0: f32, y0: f32, x1: f32, y1: f32, shade: f32) -> String {
    format!(
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#,
        num(x0),
        num(y0),
        num((x1 - x0).max(0.2)),
        num((y1 - y0).max(0.2)),
        hex(shade),
    )
}
