//! The PDF, written by hand.
//!
//! About a hundred and fifty lines, and every byte is ours: a catalogue, a page
//! tree, one content stream per page, a real cross-reference table with the byte
//! offset of every object. No fonts are embedded and none are named — the
//! content stream is nothing but filled paths, which is what makes Persian come
//! out right without a CID font in sight.
//!
//! PDF counts y upwards from the bottom of the page while the layout counts it
//! downwards from the top. That difference is settled in exactly one place:
//! each glyph's transform carries the flip (`s 0 0 -s`), so the outline itself
//! is written in font units and never has to think about it.

use super::font::{self, Seg};
use super::layout::{num, Item, Page, PAGE_H, PAGE_W};

/// Writes every page into one PDF file.
pub fn render(pages: &[Page]) -> Vec<u8> {
    let empty = Page::default();
    let pages: Vec<&Page> = if pages.is_empty() {
        vec![&empty]
    } else {
        pages.iter().collect()
    };
    let streams: Vec<String> = pages.iter().map(|page| content(page)).collect();

    let objects = 2 + streams.len() * 2;
    let mut out: Vec<u8> = Vec::with_capacity(24 * 1024 + streams.iter().map(|s| s.len()).sum::<usize>());
    let mut at = vec![0usize; objects + 1];

    out.extend_from_slice(b"%PDF-1.4\n");
    // A comment of high bytes marks the file as binary, which every reader
    // since Acrobat 1.1 expects to see near the top.
    out.extend_from_slice(b"%\xe2\xe3\xcf\xd3\n");

    plain(&mut out, &mut at, 1, "<< /Type /Catalog /Pages 2 0 R >>");

    let kids: String = (0..streams.len())
        .map(|i| format!("{} 0 R ", 3 + 2 * i))
        .collect();
    plain(
        &mut out,
        &mut at,
        2,
        &format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.trim_end(),
            streams.len()
        ),
    );

    for (i, stream) in streams.iter().enumerate() {
        let page_id = 3 + 2 * i;
        plain(
            &mut out,
            &mut at,
            page_id,
            &format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << >> /Contents {} 0 R >>",
                num(PAGE_W),
                num(PAGE_H),
                page_id + 1
            ),
        );
        stream_object(&mut out, &mut at, page_id + 1, stream);
    }

    let xref_at = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for id in 1..=objects {
        out.extend_from_slice(format!("{:010} 00000 n \n", at[id]).as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            objects + 1,
            xref_at
        )
        .as_bytes(),
    );
    out
}

fn plain(out: &mut Vec<u8>, at: &mut [usize], id: usize, body: &str) {
    at[id] = out.len();
    out.extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
}

fn stream_object(out: &mut Vec<u8>, at: &mut [usize], id: usize, stream: &str) {
    at[id] = out.len();
    out.extend_from_slice(format!("{id} 0 obj\n<< /Length {} >>\nstream\n", stream.len()).as_bytes());
    out.extend_from_slice(stream.as_bytes());
    out.extend_from_slice(b"\nendstream\nendobj\n");
}

/// One page's content stream: the glyphs as paths, then the rules as paths.
fn content(page: &Page) -> String {
    let mut out = String::with_capacity(8 * 1024);
    out.push_str("q\n");

    for item in &page.items {
        match item {
            Item::Text(text) => {
                if text.glyphs.is_empty() {
                    continue;
                }
                let scale = font::scale(text.weight, text.size);
                if scale <= 0.0 {
                    continue;
                }
                out.push_str(&format!("{} g\n", num(text.shade)));
                let s = num(scale);
                for glyph in &text.glyphs {
                    let segs = font::outline(text.weight, glyph.gid);
                    if segs.is_empty() {
                        continue;
                    }
                    // One transform per glyph: scale, flip y, and put the
                    // glyph's origin where the layout put it.
                    out.push_str("q\n");
                    out.push_str(&format!(
                        "{s} 0 0 -{s} {x} {y} cm\n",
                        x = num(text.x + glyph.dx),
                        y = num(PAGE_H - (text.baseline - glyph.dy)),
                    ));
                    outline(&mut out, &segs);
                    out.push_str("f\nQ\n");
                }
            }
            Item::Rule(rule) => {
                let half = (rule.width / 2.0).max(0.1);
                let top = PAGE_H - (rule.y - half);
                let bottom = PAGE_H - (rule.y + half);
                out.push_str(&format!("{} g\n", num(rule.shade)));
                out.push_str(&format!(
                    "{} {top} m {} {top} l {} {bottom} l {} {bottom} l h\nf\n",
                    num(rule.x0),
                    num(rule.x1),
                    num(rule.x1),
                    num(rule.x0),
                ));
            }
        }
    }

    out.push_str("Q\n");
    out
}

/// The outline with PDF's path operators. Quadratic curves — which is all a
/// TrueType font has — become cubics, because PDF has no `q` operator.
fn outline(out: &mut String, segs: &[Seg]) {
    let mut cur = (0.0f32, 0.0f32);
    for seg in segs {
        match seg {
            Seg::Move(x, y) => {
                out.push_str(&format!("{} {} m\n", num(*x), num(*y)));
                cur = (*x, *y);
            }
            Seg::Line(x, y) => {
                out.push_str(&format!("{} {} l\n", num(*x), num(*y)));
                cur = (*x, *y);
            }
            Seg::Quad(cx, cy, x, y) => {
                let c1 = (cur.0 + 2.0 / 3.0 * (cx - cur.0), cur.1 + 2.0 / 3.0 * (cy - cur.1));
                let c2 = (x + 2.0 / 3.0 * (cx - x), y + 2.0 / 3.0 * (cy - y));
                out.push_str(&format!(
                    "{} {} {} {} {} {} c\n",
                    num(c1.0),
                    num(c1.1),
                    num(c2.0),
                    num(c2.1),
                    num(*x),
                    num(*y)
                ));
                cur = (*x, *y);
            }
            Seg::Cubic(x1, y1, x2, y2, x, y) => {
                out.push_str(&format!(
                    "{} {} {} {} {} {} c\n",
                    num(*x1),
                    num(*y1),
                    num(*x2),
                    num(*y2),
                    num(*x),
                    num(*y)
                ));
                cur = (*x, *y);
            }
            Seg::Close => out.push_str("h\n"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pdf_with_no_drawing_is_still_a_pdf() {
        let bytes = render(&[]);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.ends_with("%%EOF\n"));
        assert!(text.contains("/MediaBox [0 0 420 595]"));
    }
}
