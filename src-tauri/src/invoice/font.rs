//! The font: three weights of Vazirmatn, shaped by rustybuzz, cut into outlines
//! by ttf-parser.
//!
//! The invoices carry no font program at all — every letter is a set of filled
//! paths. That is the whole trick that makes the PDF and the SVG identical and
//! keeps Persian out of the CID/RTL minefield. So shaping is the only thing this
//! module does at a high level: take a run of text and a direction, hand back
//! the glyphs in visual order with their advances in points, ready to place.
//!
//! The build of Vazirmatn under `assets/fonts` is the Farsi-Digits one: the
//! ASCII digits are mapped to the Persian digit shapes, so a total typed as
//! `5175` and a total formatted as `۵٬۱۷۵` come out looking the same. The
//! `tnum` feature is switched on so every digit occupies the same width and the
//! columns of an invoice line up.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::bidi::Dir;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Weight {
    Regular,
    Medium,
    Bold,
}

impl Weight {
    fn index(self) -> usize {
        match self {
            Weight::Regular => 0,
            Weight::Medium => 1,
            Weight::Bold => 2,
        }
    }
}

const BYTES: [&[u8]; 3] = [
    include_bytes!("../../assets/fonts/Vazirmatn-Regular.ttf"),
    include_bytes!("../../assets/fonts/Vazirmatn-Medium.ttf"),
    include_bytes!("../../assets/fonts/Vazirmatn-Bold.ttf"),
];

/// One glyph of a typeset run, already in visual order.
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    /// Glyph id in the face the run was shaped with.
    pub gid: u16,
    /// How far the glyph's origin sits from the left edge of the run, in points.
    pub dx: f32,
    /// Where the glyph sits above the baseline, in points (positive = up).
    pub dy: f32,
    /// How wide the glyph is, in points.
    pub adv: f32,
}

/// A run of text after shaping, laid out left to right.
#[derive(Clone, Debug, Default)]
pub struct Typeset {
    pub glyphs: Vec<Glyph>,
    pub width: f32,
}

/// Shapes `text` at `size` points and returns the glyphs in **visual** order:
/// left to right, whatever direction the run itself runs in.
///
/// rustybuzz hands a right-to-left run back in reverse logical order — the test
/// `right_to_left_glyphs_arrive_in_visual_order` in this module is the
/// documentation for that. Rather than trust it blindly, the order is detected
/// from the cluster indices and the offsets are computed accordingly, so a
/// change of behaviour in the dependency would still draw the right thing.
pub fn typeset(text: &str, size: f32, weight: Weight, dir: Dir) -> Typeset {
    if text.is_empty() || size <= 0.0 {
        return Typeset::default();
    }
    let Some(face) = shape_face(weight) else {
        return Typeset::default();
    };
    let scale = size / face.units_per_em() as f32;

    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.set_direction(match dir {
        Dir::Rtl => rustybuzz::Direction::RightToLeft,
        Dir::Ltr => rustybuzz::Direction::LeftToRight,
    });

    // Tabular figures: every digit the same width, so columns of money line up.
    let features = [rustybuzz::Feature::new(
        rustybuzz::ttf_parser::Tag::from_bytes(b"tnum"),
        1,
        ..,
    )];
    let shaped = rustybuzz::shape(&face, &features, buffer);
    let infos = shaped.glyph_infos();
    let positions = shaped.glyph_positions();
    if infos.is_empty() {
        return Typeset::default();
    }

    // A right-to-left run that comes back with falling clusters is already in
    // visual order; one with rising clusters is still in logical order and has
    // to be measured from the right edge instead of the left.
    let reversed =
        infos.len() > 1 && infos[0].cluster > infos[infos.len() - 1].cluster;
    let flip = dir == Dir::Rtl && !reversed;

    let mut pen = 0f32;
    let mut raw = Vec::with_capacity(infos.len());
    for (info, pos) in infos.iter().zip(positions) {
        let adv = pos.x_advance as f32 * scale;
        let off = pos.x_offset as f32 * scale;
        raw.push((info.glyph_id as u16, pen, off, pos.y_offset as f32 * scale, adv));
        pen += adv;
    }
    let width = pen;

    let mut glyphs = Vec::with_capacity(raw.len());
    for (gid, before, off, dy, adv) in raw {
        // In logical order the pen runs from the right edge, so a glyph starts
        // `width - (before + adv)` from the left instead of `before`.
        let dx = if flip { width - before - adv } else { before } + off;
        glyphs.push(Glyph { gid, dx, dy, adv });
    }

    Typeset { glyphs, width }
}

fn shape_face(weight: Weight) -> Option<rustybuzz::Face<'static>> {
    rustybuzz::Face::from_slice(BYTES[weight.index()], 0)
}

/// One segment of a glyph outline, in font units (2048 to the em), y upwards.
#[derive(Clone, Copy, Debug)]
pub enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    Quad(f32, f32, f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

/// The outline of one glyph in font units, cached for the life of the process.
pub fn outline(weight: Weight, gid: u16) -> Arc<Vec<Seg>> {
    static CACHE: OnceLock<Mutex<HashMap<(usize, u16), Arc<Vec<Seg>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (weight.index(), gid);
    if let Ok(map) = cache.lock() {
        if let Some(found) = map.get(&key) {
            return Arc::clone(found);
        }
    }
    let built = Arc::new(build_outline(weight, gid));
    if let Ok(mut map) = cache.lock() {
        map.insert(key, Arc::clone(&built));
    }
    built
}

/// Font units to points for a given size: 2048 units are an em in every weight
/// of Vazirmatn, but the number is read from the file rather than assumed.
pub fn scale(weight: Weight, size: f32) -> f32 {
    static UPEM: OnceLock<[f32; 3]> = OnceLock::new();
    let upem = UPEM.get_or_init(|| {
        let mut out = [2048.0; 3];
        for (i, bytes) in BYTES.iter().enumerate() {
            if let Ok(face) = ttf_parser::Face::parse(bytes, 0) {
                out[i] = face.units_per_em() as f32;
            }
        }
        out
    });
    size / upem[weight.index()]
}

fn build_outline(weight: Weight, gid: u16) -> Vec<Seg> {
    struct Builder {
        segs: Vec<Seg>,
    }
    impl ttf_parser::OutlineBuilder for Builder {
        fn move_to(&mut self, x: f32, y: f32) {
            self.segs.push(Seg::Move(x, y));
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.segs.push(Seg::Line(x, y));
        }
        fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
            self.segs.push(Seg::Quad(x1, y1, x, y));
        }
        fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
            self.segs.push(Seg::Cubic(x1, y1, x2, y2, x, y));
        }
        fn close(&mut self) {
            self.segs.push(Seg::Close);
        }
    }

    let Ok(face) = ttf_parser::Face::parse(BYTES[weight.index()], 0) else {
        return Vec::new();
    };
    let mut builder = Builder { segs: Vec::new() };
    face.outline_glyph(ttf_parser::GlyphId(gid), &mut builder);
    builder.segs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gids(text: &str, weight: Weight, dir: Dir) -> Vec<u16> {
        typeset(text, 10.0, weight, dir)
            .glyphs
            .iter()
            .map(|g| g.gid)
            .collect()
    }

    /// The claim the whole layout rests on, written down so it cannot drift
    /// silently: for a right-to-left run rustybuzz returns the glyphs in
    /// reverse logical order, which is visual order, left to right.
    #[test]
    fn right_to_left_glyphs_arrive_in_visual_order() {
        let face = shape_face(Weight::Regular).expect("Vazirmatn parses");
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str("سلام");
        buffer.set_direction(rustybuzz::Direction::RightToLeft);
        let shaped = rustybuzz::shape(&face, &[], buffer);
        let clusters: Vec<u32> = shaped.glyph_infos().iter().map(|i| i.cluster).collect();

        assert_eq!(
            clusters,
            vec![6, 2, 0],
            "rustybuzz now returns RTL glyphs in {clusters:?}; the layout in font.rs must follow"
        );
        // ...and what this module hands to the layout is sorted left to right.
        let placed = typeset("سلام", 10.0, Weight::Regular, Dir::Rtl);
        for pair in placed.glyphs.windows(2) {
            assert!(pair[0].dx <= pair[1].dx, "{:?}", placed.glyphs);
        }
    }

    #[test]
    fn a_left_to_right_run_is_laid_out_left_to_right() {
        let placed = typeset("1404/08/08", 10.0, Weight::Regular, Dir::Ltr);
        for pair in placed.glyphs.windows(2) {
            assert!(pair[0].dx < pair[1].dx);
        }
        assert!(placed.width > 0.0);
    }

    #[test]
    fn the_outline_of_a_letter_has_contours() {
        // «م» is glyph 1220 in Vazirmatn; whatever its id, it must have curves.
        let ids = gids("م", Weight::Regular, Dir::Rtl);
        assert_eq!(ids.len(), 1);
        let segs = outline(Weight::Regular, ids[0]);
        assert!(segs.len() > 4, "no outline for gid {}", ids[0]);
        assert!(segs.iter().any(|s| matches!(s, Seg::Quad(..))));
        assert!(segs.iter().any(|s| matches!(s, Seg::Close)));
    }

    #[test]
    fn tabular_digits_all_move_the_pen_by_the_same_amount() {
        let placed = typeset("۱۲۳۴۵۶۷۸۹۰", 10.0, Weight::Regular, Dir::Ltr);
        assert_eq!(placed.glyphs.len(), 10);
        let first = placed.glyphs[0].adv;
        for glyph in &placed.glyphs {
            assert!((glyph.adv - first).abs() < 0.001, "{:?}", placed.glyphs);
        }
    }
}
