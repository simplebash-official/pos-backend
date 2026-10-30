use super::derive_code::derive_code;

#[test]
fn derives_first_three_letters_uppercased() {
    assert_eq!(derive_code("Phone Repairs"), "PHO");
    assert_eq!(derive_code("Screens"), "SCR");
}

#[test]
fn pads_short_names_with_x() {
    assert_eq!(derive_code("TV"), "TVX");
    assert_eq!(derive_code("A"), "AXX");
}

#[test]
fn falls_back_to_all_x_for_names_without_letters() {
    assert_eq!(derive_code("24/7"), "XXX");
    assert_eq!(derive_code(""), "XXX");
}

#[test]
fn strips_punctuation_and_digits_before_taking_letters() {
    // Letters after stripped punctuation/digits still start the code,
    // even if they aren't the first *word* in the name.
    assert_eq!(derive_code("24/7 Support"), "SUP");
    assert_eq!(derive_code("Mug, T-Shirt & Print Customization"), "MUG");
}
