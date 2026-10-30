use super::generate_id::generate_id;

#[test]
fn generate_id_creates_prefixed_key() {
    let key = generate_id("prod");
    assert!(key.starts_with("prod_"));
    assert_eq!(key.len(), 5 + 16);
}

#[test]
#[should_panic(expected = "Server must never issue keys starting with 'local_'")]
fn generate_id_panics_on_local_prefix() {
    generate_id("local");
}

#[test]
#[should_panic(expected = "Server must never issue keys starting with 'local_'")]
fn generate_id_panics_on_local_underscore_prefix() {
    generate_id("local_product");
}
