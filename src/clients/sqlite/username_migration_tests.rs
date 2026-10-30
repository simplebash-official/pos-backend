use super::clients_sqlite::derive_username;

use std::collections::HashSet;

#[test]
fn derives_clean_unique_usernames() {
    let mut taken = HashSet::from(["admin".to_string()]);
    assert_eq!(derive_username("Ann.Lee@shop.com", &taken), "ann.lee");
    taken.insert("ann.lee".into());
    assert_eq!(derive_username("ann.lee@other.com", &taken), "ann.lee2");
    assert_eq!(derive_username("a@x.com", &taken), "a12");
    assert_eq!(derive_username("+bob+@x.com", &taken), "bob");
}
