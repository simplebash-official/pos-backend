use super::service_print_payload::number_to_words_rupees;

#[test]
fn matches_frontend_examples() {
    assert_eq!(
        number_to_words_rupees(8_000_000),
        "Sri Lankan Rupees Eighty Thousand Only"
    );
    assert_eq!(
        number_to_words_rupees(8_000_050),
        "Sri Lankan Rupees Eighty Thousand and Cents Fifty Only"
    );
    assert_eq!(number_to_words_rupees(0), "Sri Lankan Rupees Zero Only");
    assert_eq!(
        number_to_words_rupees(1_100_000),
        "Sri Lankan Rupees Eleven Thousand Only"
    );
}
