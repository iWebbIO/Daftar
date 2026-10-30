//! The amount written out in Persian words — «مبلغ به حروف».
//!
//! Every printed Iranian فاکتور carries the total in words under the figure,
//! and a shopkeeper can tell at a glance whether a document was made by
//! something that knows the language. So the numbers are said the way a Persian
//! speaker says them:
//!
//! ```text
//! 1_005        هزار و پنج                 (not «یک هزار و صفر و پنج»)
//! 1_862_000    یک میلیون و هشتصد و شصت و دو هزار
//! 36_075       سی و شش هزار و هفتاد و پنج
//! ```
//!
//! Digits are grouped in threes the way the script groups them, «یک» is dropped
//! before «هزار» because nobody says «یک هزار», and everything is joined with
//! «و». Nothing is padded with «صفر».

const ONES: [&str; 20] = [
    "",
    "یک",
    "دو",
    "سه",
    "چهار",
    "پنج",
    "شش",
    "هفت",
    "هشت",
    "نه",
    "ده",
    "یازده",
    "دوازده",
    "سیزده",
    "چهارده",
    "پانزده",
    "شانزده",
    "هفده",
    "هجده",
    "نوزده",
];

const TENS: [&str; 10] = [
    "", "", "بیست", "سی", "چهل", "پنجاه", "شصت", "هفتاد", "هشتاد", "نود",
];

const HUNDREDS: [&str; 10] = [
    "",
    "صد",
    "دویست",
    "سیصد",
    "چهارصد",
    "پانصد",
    "ششصد",
    "هفتصد",
    "هشتصد",
    "نهصد",
];

/// `۳۴۵` → «سیصد و چهل و پنج».
fn under_thousand(n: u32) -> String {
    let mut parts: Vec<&str> = Vec::new();
    let hundreds = (n / 100) as usize;
    let rest = n % 100;
    if hundreds > 0 {
        parts.push(HUNDREDS[hundreds]);
    }
    if rest > 0 {
        if rest < 20 {
            parts.push(ONES[rest as usize]);
        } else {
            parts.push(TENS[(rest / 10) as usize]);
            let ones = rest % 10;
            if ones > 0 {
                parts.push(ONES[ones as usize]);
            }
        }
    }
    parts.join(" و ")
}

fn magnitude(value: u64) -> String {
    const MILLIARD: u64 = 1_000_000_000;
    const MILLION: u64 = 1_000_000;
    const THOUSAND: u64 = 1_000;

    if value >= MILLIARD {
        let head = format!("{} میلیارد", magnitude(value / MILLIARD));
        let rest = value % MILLIARD;
        return if rest == 0 {
            head
        } else {
            format!("{head} و {}", magnitude(rest))
        };
    }
    if value >= MILLION {
        let head = format!("{} میلیون", magnitude(value / MILLION));
        let rest = value % MILLION;
        return if rest == 0 {
            head
        } else {
            format!("{head} و {}", magnitude(rest))
        };
    }
    if value >= THOUSAND {
        let count = value / THOUSAND;
        let rest = value % THOUSAND;
        // Exactly a thousand is «هزار»; a thousand and something keeps its
        // «یک», the way it is written on a cheque: «یک هزار و پانصد».
        let head = if count == 1 && rest == 0 {
            "هزار".to_string()
        } else {
            format!("{} هزار", magnitude(count))
        };
        return if rest == 0 {
            head
        } else {
            format!("{head} و {}", magnitude(rest))
        };
    }
    under_thousand(value as u32)
}

/// A whole number of toman in Persian words, without the unit.
pub fn words(n: i64) -> String {
    if n == 0 {
        return "صفر".to_string();
    }
    let said = magnitude(n.unsigned_abs());
    if n < 0 {
        format!("منفی {said}")
    } else {
        said
    }
}

/// The line that goes under the total: `۱٬۸۶۲٬۰۰۰` → «یک میلیون و هشتصد و
/// شصت و دو هزار تومان».
pub fn toman(n: i64) -> String {
    format!("{} تومان", words(n))
}

/// The line that goes under the total: `۱٬۸۶۲٬۰۰۰` → «یک میلیون و هشتصد و
/// شصت و دو هزار تومان». The ledger is in toman; a shop that prints in rial
/// gets the same amount at ten times the figure, with the other unit word.
pub fn currency(toman: i64, currency: &str) -> String {
    let printed = toman.saturating_mul(crate::money::scale(currency));
    format!("{} {}", words(printed), crate::money::unit(currency))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_numbers_a_shop_says() {
        assert_eq!(words(0), "صفر");
        assert_eq!(words(1), "یک");
        assert_eq!(words(5), "پنج");
        assert_eq!(words(100), "صد");
        assert_eq!(words(1_000), "هزار");
        assert_eq!(words(1_005), "یک هزار و پنج");
        assert_eq!(words(12_345), "دوازده هزار و سیصد و چهل و پنج");
        assert_eq!(words(100_000), "صد هزار");
        assert_eq!(words(1_000_000), "یک میلیون");
        assert_eq!(words(1_862_000), "یک میلیون و هشتصد و شصت و دو هزار");
        assert_eq!(words(36_075), "سی و شش هزار و هفتاد و پنج");
        assert_eq!(
            words(999_999_999),
            "نهصد و نود و نه میلیون و نهصد و نود و نه هزار و نهصد و نود و نه"
        );
    }

    #[test]
    fn the_examples_from_the_invoice() {
        // The two forms the Lead put on the table.
        assert_eq!(words(1_500_000), "یک میلیون و پانصد هزار");
        assert_eq!(
            words(2_250_500),
            "دو میلیون و دویست و پنجاه هزار و پانصد"
        );
    }

    #[test]
    fn a_large_figure_still_reads_as_persian() {
        assert_eq!(
            words(1_234_567_890_123),
            "یک هزار و دویست و سی و چهار میلیارد و پانصد و شصت و هفت میلیون \
             و هشتصد و نود هزار و صد و بیست و سه"
        );
    }

    #[test]
    fn the_teens_and_the_tens_join_correctly() {
        assert_eq!(words(19), "نوزده");
        assert_eq!(words(21), "بیست و یک");
        assert_eq!(words(345), "سیصد و چهل و پنج");
        assert_eq!(words(999), "نهصد و نود و نه");
        assert_eq!(words(1_100_000), "یک میلیون و صد هزار");
    }

    #[test]
    fn a_debt_is_said_as_a_negative() {
        assert_eq!(words(-2_500), "منفی دو هزار و پانصد");
        assert_eq!(toman(1_862_000), "یک میلیون و هشتصد و شصت و دو هزار تومان");
    }

    #[test]
    fn the_same_amount_is_ten_times_the_figure_in_rial() {
        assert_eq!(currency(1_862_000, "toman"), "یک میلیون و هشتصد و شصت و دو هزار تومان");
        assert_eq!(
            currency(1_862_000, "rial"),
            "هجده میلیون و ششصد و بیست هزار ریال"
        );
        assert_eq!(currency(0, "rial"), "صفر ریال");
        assert_eq!(currency(6_405, "toman"), "شش هزار و چهارصد و پنج تومان");
        assert_eq!(currency(6_405, "rial"), "شصت و چهار هزار و پنجاه ریال");
    }
}
