use super::all::all;
use super::find::find;
use super::starter_key_prefix::STARTER_KEY_PREFIX;

use crate::domain::labels::LabelTemplateInput;
use crate::modules::labels::service::validate::{check_code_fits, validate_template};

#[test]
fn every_starter_is_a_valid_design_whose_codes_fit() {
    let starters = all();
    assert_eq!(starters.len(), 3);
    for template in &starters {
        let input = LabelTemplateInput {
            name: template.name.clone(),
            width_mm: template.width_mm,
            height_mm: template.height_mm,
            elements: template.elements.clone(),
        };
        validate_template(&input).unwrap_or_else(|e| panic!("{}: {e}", template.name));
        // The longest code the app issues: a 14-digit manual barcode.
        check_code_fits(&template.elements, "20000000000150", "x")
            .unwrap_or_else(|e| panic!("{}: {e}", template.name));
        assert!(template.built_in && template.key.starts_with(STARTER_KEY_PREFIX));
    }
}

#[test]
fn starter_keys_and_names_are_unique() {
    let starters = all();
    for (i, a) in starters.iter().enumerate() {
        for b in &starters[i + 1..] {
            assert_ne!(a.key, b.key);
            assert_ne!(a.name.to_lowercase(), b.name.to_lowercase());
        }
    }
    assert!(find("starter_shelf_58x40").is_some());
    assert!(find("starter_missing").is_none());
}
