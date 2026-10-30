//! Money and quantity formatting.
//!
//! Everything the shopkeeper sees is Persian digits with Persian separators:
//! `۳۴۵٬۰۰۰` for money and `۲٫۵` for two and a half kilos. Parsing accepts
//! whatever the phone keyboard produces — Persian, Arabic-Indic or Latin digits,
//! with or without separators — because nobody should have to think about it.

/// U+066C ARABIC THOUSANDS SEPARATOR
pub const THOUSANDS: char = '٬';
/// U+066B ARABIC DECIMAL SEPARATOR
pub const DECIMAL: char = '٫';

/// Rewrites any digit in the string as a Persian digit.
pub fn fa_digits(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '0'..='9' => char::from_u32('۰' as u32 + (c as u32 - '0' as u32)).unwrap_or(c),
            '\u{0660}'..='\u{0669}' => {
                char::from_u32('۰' as u32 + (c as u32 - '\u{0660}' as u32)).unwrap_or(c)
            }
            _ => c,
        })
        .collect()
}

/// The numeric value of any digit character, Persian, Arabic-Indic or Latin.
pub fn digit_value(c: char) -> Option<u32> {
    match c {
        '0'..='9' => Some(c as u32 - '0' as u32),
        '\u{06F0}'..='\u{06F9}' => Some(c as u32 - '\u{06F0}' as u32),
        '\u{0660}'..='\u{0669}' => Some(c as u32 - '\u{0660}' as u32),
        _ => None,
    }
}

/// `۳۴۵٬۰۰۰`
pub fn fmt_amount(n: i64) -> String {
    let negative = n < 0;
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(THOUSANDS);
        }
        out.push(ch);
    }
    let out = fa_digits(&out);
    if negative {
        format!("−{out}")
    } else {
        out
    }
}

/// `۳۴۵٬۰۰۰ تومان`
pub fn fmt_toman(n: i64) -> String {
    format!("{} تومان", fmt_amount(n))
}

/// Thousandths to a readable quantity: 2500 -> `۲٫۵`, 1000 -> `۱`, 1234 -> `۱٫۲۳۴`.
pub fn fmt_qty(milli: i64) -> String {
    let whole = milli / 1000;
    let frac = (milli % 1000).abs();
    if frac == 0 {
        return fa_digits(&whole.to_string());
    }
    let mut s = format!("{whole}.{frac:03}");
    while s.ends_with('0') {
        s.pop();
    }
    fa_digits(&s).replace('.', &DECIMAL.to_string())
}

/// Parses a typed amount. Accepts `۳۴۵٬۰۰۰`, `345,000`, `345 000`, `345000`,
/// and the shorthand people actually write: `۳۴۵ هزار` and `۳۴۵ه`.
pub fn parse_amount(input: &str) -> Option<i64> {
    let mut cleaned = String::new();
    let mut scale: i64 = 1;
    let mut text = input.trim().replace('\u{200c}', "");
    for suffix in ["هزار", "ه"] {
        if text.ends_with(suffix) && !text.trim_end_matches(suffix).trim().is_empty() {
            scale = 1_000;
            text = text.trim_end_matches(suffix).trim().to_string();
            break;
        }
    }
    for c in text.chars() {
        if let Some(d) = digit_value(c) {
            cleaned.push(char::from_digit(d, 10)?);
        } else if c.is_whitespace() || c == THOUSANDS || c == ',' || c == '_' || c == '\u{066A}' {
            continue;
        } else {
            return None;
        }
    }
    if cleaned.is_empty() {
        return None;
    }
    cleaned.parse::<i64>().ok().map(|n| n * scale)
}

/// Parses a typed quantity into thousandths. `۲٫۵` and `2.5` both mean 2500.
pub fn parse_qty(input: &str) -> Option<i64> {
    let mut cleaned = String::new();
    let mut seen_sep = false;
    for c in input.trim().chars() {
        if let Some(d) = digit_value(c) {
            cleaned.push(char::from_digit(d, 10)?);
        } else if c == DECIMAL || c == '.' {
            if seen_sep {
                return None;
            }
            seen_sep = true;
            cleaned.push('.');
        } else if c == THOUSANDS || c == ',' || c.is_whitespace() {
            continue;
        } else {
            return None;
        }
        let _ = &mut seen_sep;
    }
    if cleaned.is_empty() {
        return None;
    }
    let value: f64 = cleaned.parse().ok()?;
    if !value.is_finite() {
        return None;
    }
    Some((value * 1000.0).round() as i64)
}

/// Rounds an amount to the nearest `step` (50, 100, 1000 …). The difference is
/// what gets stored as `doc.rounding`, so an invoice always adds up in public.
pub fn round_to(amount: i64, step: i64) -> i64 {
    if step <= 1 {
        return amount;
    }
    let half = step / 2;
    ((amount + half).div_euclid(step)) * step
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_read_as_persian() {
        assert_eq!(fmt_amount(0), "۰");
        assert_eq!(fmt_amount(345), "۳۴۵");
        assert_eq!(fmt_amount(5_175), "۵٬۱۷۵");
        assert_eq!(fmt_amount(1_234_567), "۱٬۲۳۴٬۵۶۷");
        assert_eq!(fmt_amount(-2_500), "−۲٬۵۰۰");
    }

    #[test]
    fn every_digit_style_parses() {
        assert_eq!(parse_amount("۳۴۵٬۰۰۰"), Some(345_000));
        assert_eq!(parse_amount("345,000"), Some(345_000));
        assert_eq!(parse_amount("345 000"), Some(345_000));
        assert_eq!(parse_amount("٣٤٥"), Some(345));
        assert_eq!(parse_amount("۳۴۵ هزار"), Some(345_000));
        assert_eq!(parse_amount("۳۴۵ه"), Some(345_000));
        assert_eq!(parse_amount(""), None);
        assert_eq!(parse_amount("abc"), None);
    }

    #[test]
    fn quantities_keep_three_places() {
        assert_eq!(parse_qty("۲٫۵"), Some(2_500));
        assert_eq!(parse_qty("2.5"), Some(2_500));
        assert_eq!(parse_qty("12"), Some(12_000));
        assert_eq!(parse_qty("0.125"), Some(125));
        assert_eq!(fmt_qty(2_500), "۲٫۵");
        assert_eq!(fmt_qty(1_000), "۱");
        assert_eq!(fmt_qty(1_234), "۱٫۲۳۴");
        assert_eq!(fmt_qty(125), "۰٫۱۲۵");
    }

    #[test]
    fn rounding_is_neutral_on_totals() {
        assert_eq!(round_to(5_175, 100), 5_200);
        assert_eq!(round_to(5_120, 50), 5_100);
        assert_eq!(round_to(5_175, 1), 5_175);
        assert_eq!(round_to(5_175, 0), 5_175);
    }

    #[test]
    fn line_maths_matches_the_notebook() {
        // 15 kg × 345 = 5,175 — the first line of the 30 June page.
        let qty = parse_qty("15").unwrap();
        let line = qty * 345 / 1000;
        assert_eq!(line, 5_175);
        assert_eq!(fmt_amount(line), "۵٬۱۷۵");
    }
}
