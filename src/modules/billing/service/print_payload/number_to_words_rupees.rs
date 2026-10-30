/// Rust port of the frontend's `numberToWordsRupees` (`src/shared/lib/
/// numberToWords.ts`) — same lakh/thousand (Indian numbering system)
/// wording, so an invoice's printed amount-in-words is identical whichever
/// side renders it. `CURRENCY.decimals = 0` app-wide (see `formatMoney`),
/// but this mirrors the frontend's cents-aware wording exactly rather than
/// assuming whole rupees, since the source of truth is the TS function, not
/// the currency's display rounding.
pub(crate) fn number_to_words_rupees(amount_cents: i64) -> String {
    if amount_cents <= 0 {
        return "Sri Lankan Rupees Zero Only".to_string();
    }

    let total_rupees = amount_cents / 100;
    let cents = amount_cents % 100;

    let rupee_text = convert_number(total_rupees);
    let mut result = format!("Sri Lankan Rupees {rupee_text}");

    if cents > 0 {
        let cents_text = convert_below_thousand(cents);
        result.push_str(&format!(" and Cents {cents_text}"));
    }

    result.push_str(" Only");
    result
}

const ONES: [&str; 20] = [
    "",
    "One",
    "Two",
    "Three",
    "Four",
    "Five",
    "Six",
    "Seven",
    "Eight",
    "Nine",
    "Ten",
    "Eleven",
    "Twelve",
    "Thirteen",
    "Fourteen",
    "Fifteen",
    "Sixteen",
    "Seventeen",
    "Eighteen",
    "Nineteen",
];

const TENS: [&str; 10] = [
    "", "", "Twenty", "Thirty", "Forty", "Fifty", "Sixty", "Seventy", "Eighty", "Ninety",
];

fn convert_below_thousand(n: i64) -> String {
    if n == 0 {
        return String::new();
    }
    if n < 20 {
        return ONES[n as usize].to_string();
    }
    if n < 100 {
        let rem = n % 10;
        let tens_word = TENS[(n / 10) as usize];
        return if rem > 0 {
            format!("{tens_word} {}", ONES[rem as usize])
        } else {
            tens_word.to_string()
        };
    }
    let hundred_rem = n % 100;
    let hundreds_word = ONES[(n / 100) as usize];
    if hundred_rem > 0 {
        format!(
            "{hundreds_word} Hundred {}",
            convert_below_thousand(hundred_rem)
        )
    } else {
        format!("{hundreds_word} Hundred")
    }
}

fn convert_number(n: i64) -> String {
    if n == 0 {
        return "Zero".to_string();
    }

    let lakh = n / 100_000;
    let mut remainder = n % 100_000;
    let thousand = remainder / 1000;
    remainder %= 1000;

    let mut parts: Vec<String> = Vec::new();
    if lakh > 0 {
        parts.push(format!("{} Lakh", convert_below_thousand(lakh)));
    }
    if thousand > 0 {
        parts.push(format!("{} Thousand", convert_below_thousand(thousand)));
    }
    if remainder > 0 {
        parts.push(convert_below_thousand(remainder));
    }

    parts.join(" ")
}
