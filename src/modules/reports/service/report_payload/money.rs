/// `12345` -> `"Rs. 123.45"`, `-500000` -> `"-Rs. 5,000.00"`.
pub(super) fn money(cents: i64) -> String {
    let neg = cents < 0;
    let abs = cents.unsigned_abs();
    let rupees = abs / 100;
    let paisa = abs % 100;
    // Group the integer part with thousands separators.
    let digits = rupees.to_string();
    let mut grouped = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{}Rs. {}.{:02}", if neg { "-" } else { "" }, grouped, paisa)
}
