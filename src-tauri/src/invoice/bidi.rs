//! Just enough bidi for a shop invoice.
//!
//! This is not the Unicode Bidirectional Algorithm and it does not pretend to be
//! — but the part of it that a Persian invoice needs is here:
//!
//! * Persian and Arabic letters are right-to-left.
//! * Latin letters are left-to-right.
//! * Digits are numbers, and a number is written left-to-right even inside a
//!   Persian line, which is why «۵٬۱۷۵» never comes out mirrored.
//! * `٬ ٫ , . / : -` are *number separators*: they belong to a number when a
//!   digit is on one side of them («۱۴۰۴/۰۸/۰۸», «۰۲۱-۱۲۳۴۵۶۷», «−۵٬۱۷۵») and
//!   are ordinary punctuation otherwise («خریدار: لیلا امینی», where the colon
//!   has to stay glued to the word on its right).
//! * Everything else (spaces, `، ؛ ؟ ! ( ) « »` and the zero-width non-joiner)
//!   is neutral and takes the direction of the last strong character before it,
//!   or the paragraph direction at the start of a line.
//!
//! A line becomes a list of [`Run`]s, each with one direction. The layout shapes
//! every run on its own and then places them along the line: for a
//! right-to-left paragraph the runs are laid down from the right edge in logical
//! order, so the first thing written is the first thing seen.
//!
//! What this deliberately does not do: explicit embeddings and isolates (`RLM`,
//! `LRE`, `PDF`), bracket mirroring, and the weak-type rules for a Latin word
//! glued straight onto a Persian one with no space between. An invoice has no
//! use for any of them, and a wrong guess there would be worse than the simple
//! rule above.

/// The direction of a run, or of a whole paragraph.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    Rtl,
    Ltr,
}

impl Dir {
    /// The paragraph direction for prose: Persian invoices are right-to-left.
    pub const PARAGRAPH: Dir = Dir::Rtl;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    Rtl,
    Ltr,
    Num,
    /// `٬ ٫ , . / : -` — a number only if it touches a digit.
    Sep,
    Neutral,
}

fn classify(c: char) -> Class {
    match c {
        // Persian, Arabic-Indic and Latin digits.
        '0'..='9' | '\u{0660}'..='\u{0669}' | '\u{06F0}'..='\u{06F9}' => Class::Num,
        '٬' | '٫' | ',' | '.' | '/' | ':' | '-' | '\u{2212}' | '\u{066A}' => Class::Sep,
        // Latin, which on an invoice is a product code or a unit someone typed
        // in English.
        'a'..='z' | 'A'..='Z' | '\u{00C0}'..='\u{02AF}' => Class::Ltr,
        // Persian and Arabic letters, including the presentation forms.
        '\u{0621}'..='\u{063A}'
        | '\u{0641}'..='\u{064A}'
        | '\u{066E}'..='\u{06D3}'
        | '\u{0750}'..='\u{077F}'
        | '\u{FB50}'..='\u{FDFF}'
        | '\u{FE70}'..='\u{FEFF}' => Class::Rtl,
        _ => Class::Neutral,
    }
}

fn is_digit(c: char) -> bool {
    classify(c) == Class::Num
}

/// One stretch of a line that shares a single direction.
#[derive(Clone, Copy, Debug)]
pub struct Run<'a> {
    pub text: &'a str,
    pub dir: Dir,
}

/// Splits a line into directional runs.
///
/// The runs come back in *logical* order — the order they were written — and
/// each borrows its slice of the original string, so a zero-width non-joiner
/// stays exactly where the shopkeeper put it and the shaper still sees a whole
/// word.
pub fn runs(text: &str, para: Dir) -> Vec<Run<'_>> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out: Vec<Run<'_>> = Vec::new();
    let mut last = para;
    let mut start = 0usize;
    let mut cur: Option<Dir> = None;

    for (n, &(i, c)) in chars.iter().enumerate() {
        let dir = match classify(c) {
            Class::Rtl => {
                last = Dir::Rtl;
                Dir::Rtl
            }
            Class::Ltr | Class::Num => {
                last = Dir::Ltr;
                Dir::Ltr
            }
            Class::Sep => {
                // A separator touching a digit is part of that number;
                // otherwise it is punctuation and follows the text around it.
                let before = n.checked_sub(1).map(|p| chars[p].1);
                let after = chars.get(n + 1).map(|p| p.1);
                if before.is_some_and(is_digit) || after.is_some_and(is_digit) {
                    last = Dir::Ltr;
                    Dir::Ltr
                } else {
                    last
                }
            }
            Class::Neutral => last,
        };
        match cur {
            None => {
                cur = Some(dir);
                start = i;
            }
            Some(open) if open != dir => {
                out.push(Run {
                    text: &text[start..i],
                    dir: open,
                });
                cur = Some(dir);
                start = i;
            }
            Some(_) => {}
        }
    }

    if let Some(dir) = cur {
        out.push(Run {
            text: &text[start..],
            dir,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(text: &str) -> Vec<(Dir, &str)> {
        runs(text, Dir::PARAGRAPH)
            .into_iter()
            .map(|r| (r.dir, r.text))
            .collect()
    }

    #[test]
    fn a_number_is_its_own_left_to_right_run() {
        assert_eq!(
            dirs("جمع کل ۵٬۱۷۵ تومان"),
            vec![
                (Dir::Rtl, "جمع کل "),
                (Dir::Ltr, "۵٬۱۷۵ "),
                (Dir::Rtl, "تومان"),
            ]
        );
    }

    #[test]
    fn a_colon_between_a_word_and_a_space_belongs_to_the_word() {
        // «خریدار: لیلا امینی» — the colon after a letter is punctuation, so it
        // stays glued to «خریدار» instead of jumping over to the number side.
        assert_eq!(
            dirs("خریدار: لیلا امینی"),
            vec![(Dir::Rtl, "خریدار: لیلا امینی")]
        );
        // ...and one that touches a digit is part of the number.
        assert_eq!(
            dirs("مانده حساب: ۱۰٬۸۷۵ تومان"),
            vec![
                (Dir::Rtl, "مانده حساب: "),
                (Dir::Ltr, "۱۰٬۸۷۵ "),
                (Dir::Rtl, "تومان"),
            ]
        );
    }

    #[test]
    fn a_zero_width_non_joiner_never_breaks_a_word() {
        // «صورت‌حساب» has to reach the shaper as one word or the shaping is
        // wrong; the ZWNJ is neutral, so it stays inside the run.
        assert_eq!(dirs("صورت‌حساب"), vec![(Dir::Rtl, "صورت‌حساب")]);
    }

    #[test]
    fn latin_product_codes_are_left_to_right() {
        // The Latin word and the digit that follows it are both left-to-right,
        // so they travel together inside one run and never get reordered.
        assert_eq!(
            dirs("برنج Thai ۵ کیلو"),
            vec![
                (Dir::Rtl, "برنج "),
                (Dir::Ltr, "Thai ۵ "),
                (Dir::Rtl, "کیلو"),
            ]
        );
    }

    #[test]
    fn a_whole_line_of_digits_is_one_left_to_right_run() {
        assert_eq!(dirs("۱۴۰۴/۰۸/۰۸"), vec![(Dir::Ltr, "۱۴۰۴/۰۸/۰۸")]);
        assert_eq!(dirs("۰۲۱-۱۲۳۴۵۶۷"), vec![(Dir::Ltr, "۰۲۱-۱۲۳۴۵۶۷")]);
    }

    #[test]
    fn a_negative_amount_keeps_its_sign_glued_on_the_left() {
        // `fmt_amount` writes −۵٬۱۷۵ with U+2212, which must not become a
        // separate neutral run: it would jump to the wrong side of the number.
        assert_eq!(dirs("−۵٬۱۷۵"), vec![(Dir::Ltr, "−۵٬۱۷۵")]);
    }

    #[test]
    fn silence_is_one_run_and_on_the_paragraph_side() {
        assert_eq!(dirs(""), Vec::new());
        assert_eq!(dirs("   "), vec![(Dir::Rtl, "   ")]);
    }
}
