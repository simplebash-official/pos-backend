use super::cache_key::cache_key;
use super::design_settings::DesignSettings;
use super::list_in::list_in;
use super::merge_settings_in::merge_settings_in;
use super::option_values::OptionValues;
use super::resolve_in::resolve_in;
use super::selections::Selections;
use super::validate_option_value::validate_option_value;
use super::with_options::with_options;
use crate::clients::document_server::TemplateInfo;
use crate::core::error::WireParts;

use crate::domain::settings::{DesignOption, DesignOptionType};

use std::collections::HashMap;

use serde_json::json;

use crate::domain::settings::DesignOptionChoice;

fn template(name: &str, family: &str, title: &str, kind: &str, is_default: bool) -> TemplateInfo {
    TemplateInfo {
        key: format!("tpl_{name}"),
        name: name.to_string(),
        family: family.to_string(),
        version: 1,
        kind: kind.to_string(),
        title: title.to_string(),
        r#type: "document".to_string(),
        scope: "global".to_string(),
        is_default,
        options: Vec::new(),
        data_schema: json!({}),
        sample: json!({}),
        sha256: String::new(),
        min_app_version: None,
    }
}

fn with_options_declared(mut info: TemplateInfo) -> TemplateInfo {
    info.scope = "private".into();
    info.options = vec![
        DesignOption {
            key: "showSign".into(),
            r#type: DesignOptionType::Toggle,
            label: "Signature box".into(),
            default: json!(true),
            choices: None,
            max_length: None,
        },
        DesignOption {
            key: "accent".into(),
            r#type: DesignOptionType::Select,
            label: "Accent".into(),
            default: json!("blue"),
            choices: Some(vec![
                DesignOptionChoice {
                    value: "blue".into(),
                    label: "Blue".into(),
                },
                DesignOptionChoice {
                    value: "red".into(),
                    label: "Red".into(),
                },
            ]),
            max_length: None,
        },
    ];
    info
}

fn catalog() -> Vec<TemplateInfo> {
    vec![
        template(
            "doc_temp_modern",
            "doc_temp_modern",
            "A4 Invoice Modern",
            "a4-invoice",
            false,
        ),
        template(
            "doc_temp_classic",
            "doc_temp_classic",
            "A4 Invoice",
            "a4-invoice",
            true,
        ),
        with_options_declared(template(
            "doc_temp_client2",
            "fam_client",
            "Client Invoice",
            "a4-invoice",
            false,
        )),
        template(
            "doc_temp_receipt",
            "doc_temp_receipt",
            "Thermal Receipt",
            "thermal-receipt",
            true,
        ),
        template(
            "doc_temp_credit",
            "doc_temp_credit",
            "Credit Note",
            "credit-note",
            true,
        ),
        template(
            "doc_temp_quote",
            "doc_temp_quote",
            "Quotation",
            "quotation",
            true,
        ),
        template(
            "doc_temp_report",
            "doc_temp_report",
            "Analytics Report",
            "analytics-report",
            true,
        ),
        template(
            "doc_temp_zreport",
            "doc_temp_zreport",
            "Z-Report",
            "z-report",
            true,
        ),
    ]
}

fn picks(kind: &str, family: &str) -> DesignSettings {
    DesignSettings {
        selections: Selections::from([(kind.to_string(), family.to_string())]),
        options: OptionValues::new(),
    }
}

#[test]
fn no_choice_means_the_built_in_default() {
    let resolved = resolve_in(&catalog(), "a4-invoice", None, &DesignSettings::default()).unwrap();
    assert_eq!(resolved.info.name, "doc_temp_classic");
    assert!(resolved.is_default);
}

#[test]
fn the_shops_family_is_used_and_is_not_the_default() {
    let resolved = resolve_in(
        &catalog(),
        "a4-invoice",
        None,
        &picks("a4-invoice", "fam_client"),
    )
    .unwrap();
    assert_eq!(resolved.info.name, "doc_temp_client2");
    assert!(!resolved.is_default);
}

#[test]
fn an_explicit_request_beats_the_shops_choice() {
    let resolved = resolve_in(
        &catalog(),
        "a4-invoice",
        Some("doc_temp_modern"),
        &picks("a4-invoice", "fam_client"),
    )
    .unwrap();
    assert_eq!(resolved.info.name, "doc_temp_modern");
}

#[test]
fn an_unavailable_choice_or_request_is_refused_not_replaced() {
    for (requested, settings) in [
        (Some("doc_temp_gone"), DesignSettings::default()),
        (None, picks("a4-invoice", "fam_revoked")),
        // A family of another kind is not a design for this one.
        (Some("doc_temp_receipt"), DesignSettings::default()),
    ] {
        let err = resolve_in(&catalog(), "a4-invoice", requested, &settings)
            .expect_err("must be refused");
        let WireParts { status, code, .. } = err.wire_parts();
        assert_eq!(status.as_u16(), 422);
        assert_eq!(code, "TEMPLATE_UNAVAILABLE");
    }
}

#[test]
fn a_kind_without_a_published_default_is_a_server_fault() {
    let only_modern = vec![catalog().remove(0)];
    assert!(resolve_in(&only_modern, "a4-invoice", None, &DesignSettings::default()).is_err());
}

#[test]
fn cache_key_is_historic_for_the_default_and_per_template_otherwise() {
    let default = resolve_in(&catalog(), "a4-invoice", None, &DesignSettings::default()).unwrap();
    assert_eq!(cache_key("a4-invoice", &default), "a4-invoice");
    let other = resolve_in(
        &catalog(),
        "a4-invoice",
        Some("doc_temp_modern"),
        &DesignSettings::default(),
    )
    .unwrap();
    assert_eq!(
        cache_key("a4-invoice", &other),
        "a4-invoice--doc_temp_modern"
    );
}

#[test]
fn listing_has_the_default_first_and_marks_an_unavailable_choice() {
    let settings = picks("a4-invoice", "fam_revoked");
    let listing = list_in(&catalog(), &settings).unwrap();
    let a4 = &listing[0];
    assert_eq!(a4.kind, "a4-invoice");
    assert_eq!(a4.families[0].family, "doc_temp_classic");
    assert!(a4.families[0].is_default);
    assert_eq!(
        a4.families
            .iter()
            .map(|f| f.title.as_str())
            .collect::<Vec<_>>(),
        ["A4 Invoice", "A4 Invoice Modern", "Client Invoice"]
    );
    assert_eq!(a4.selected.as_deref(), Some("fam_revoked"));
    assert_eq!(
        a4.effective, None,
        "the stored family is gone: nothing is in force"
    );
    // The other kinds are unaffected and report their default.
    assert_eq!(listing[1].effective.as_deref(), Some("doc_temp_receipt"));
}

#[test]
fn option_values_default_to_the_designs_own_defaults_and_overlay_saved_ones() {
    let saved: OptionValues = OptionValues::from([(
        "fam_client".to_string(),
        HashMap::from([("showSign".to_string(), json!(false))]),
    )]);
    let settings = DesignSettings {
        options: saved,
        ..Default::default()
    };
    let listing = list_in(&catalog(), &settings).unwrap();
    let values = &listing[0].option_values["fam_client"];
    assert_eq!(values["showSign"], json!(false));
    assert_eq!(values["accent"], json!("blue"));
    assert!(!listing[0].option_values.contains_key("doc_temp_classic"));
}

#[test]
fn with_options_adds_the_object_only_for_designs_that_declare_options() {
    let plain = resolve_in(&catalog(), "a4-invoice", None, &DesignSettings::default()).unwrap();
    let data = with_options(json!({"a": 1}), &plain).unwrap();
    assert_eq!(data, json!({"a": 1}));

    let client = resolve_in(
        &catalog(),
        "a4-invoice",
        Some("fam_client"),
        &DesignSettings::default(),
    )
    .unwrap();
    let data = with_options(json!({"a": 1}), &client).unwrap();
    assert_eq!(data["options"], json!({"showSign": true, "accent": "blue"}));
}

#[test]
fn changing_an_option_changes_the_cache_key() {
    let key_for = |accent: &str| {
        let settings = DesignSettings {
            selections: Selections::new(),
            options: OptionValues::from([(
                "fam_client".to_string(),
                HashMap::from([("accent".to_string(), json!(accent))]),
            )]),
        };
        let resolved = resolve_in(&catalog(), "a4-invoice", Some("fam_client"), &settings).unwrap();
        cache_key("a4-invoice", &resolved)
    };
    assert_ne!(key_for("blue"), key_for("red"));
    assert_eq!(key_for("red"), key_for("red"));
    assert!(key_for("red").starts_with("a4-invoice--doc_temp_client2--"));
}

#[test]
fn a_saved_value_that_no_longer_fits_the_design_is_refused() {
    let settings = DesignSettings {
        selections: Selections::new(),
        options: OptionValues::from([(
            "fam_client".to_string(),
            HashMap::from([("accent".to_string(), json!("green"))]),
        )]),
    };
    let err =
        resolve_in(&catalog(), "a4-invoice", Some("fam_client"), &settings).expect_err("refused");
    assert_eq!(err.wire_parts().code, "TEMPLATE_OPTIONS_INVALID");
}

#[test]
fn merging_validates_selections_and_options_and_changes_nothing_on_failure() {
    let update = HashMap::from([("a4-invoice".to_string(), Some("fam_client".to_string()))]);
    let options = OptionValues::from([(
        "fam_client".to_string(),
        HashMap::from([("accent".to_string(), json!("red"))]),
    )]);
    let merged =
        merge_settings_in(&catalog(), DesignSettings::default(), &update, &options).unwrap();
    assert_eq!(merged.selections["a4-invoice"], "fam_client");
    assert_eq!(merged.options["fam_client"]["accent"], json!("red"));

    // Back to the standard design.
    let cleared = HashMap::from([("a4-invoice".to_string(), None)]);
    let after = merge_settings_in(&catalog(), merged, &cleared, &OptionValues::new()).unwrap();
    assert!(after.selections.is_empty());

    let bad_options = [
        json!("purple"), // not a listed choice
        json!(true),     // wrong type
    ];
    for value in bad_options {
        let options = OptionValues::from([(
            "fam_client".to_string(),
            HashMap::from([("accent".to_string(), value)]),
        )]);
        let err = merge_settings_in(
            &catalog(),
            DesignSettings::default(),
            &HashMap::new(),
            &options,
        )
        .expect_err("refused");
        assert_eq!(err.wire_parts().code, "TEMPLATE_OPTIONS_INVALID");
    }
    let unknown_key = OptionValues::from([(
        "fam_client".to_string(),
        HashMap::from([("nope".to_string(), json!(1))]),
    )]);
    assert!(
        merge_settings_in(
            &catalog(),
            DesignSettings::default(),
            &HashMap::new(),
            &unknown_key
        )
        .is_err()
    );
    let unknown_family = OptionValues::from([("fam_none".to_string(), HashMap::new())]);
    assert!(
        merge_settings_in(
            &catalog(),
            DesignSettings::default(),
            &HashMap::new(),
            &unknown_family
        )
        .is_err()
    );
    let unknown_kind = HashMap::from([("poster".to_string(), None)]);
    assert!(
        merge_settings_in(
            &catalog(),
            DesignSettings::default(),
            &unknown_kind,
            &OptionValues::new()
        )
        .is_err()
    );
    // A family offered for a different kind cannot be picked here.
    let wrong_kind = HashMap::from([(
        "a4-invoice".to_string(),
        Some("doc_temp_receipt".to_string()),
    )]);
    assert!(
        merge_settings_in(
            &catalog(),
            DesignSettings::default(),
            &wrong_kind,
            &OptionValues::new()
        )
        .is_err()
    );
}

#[test]
fn text_and_colour_options_are_checked() {
    let colour = DesignOption {
        key: "accent".into(),
        r#type: DesignOptionType::Color,
        label: "Accent".into(),
        default: json!("#112233"),
        choices: None,
        max_length: None,
    };
    assert!(validate_option_value(&colour, &json!("#AABBCC")).is_ok());
    assert!(validate_option_value(&colour, &json!("blue")).is_err());
    let text = DesignOption {
        key: "note".into(),
        r#type: DesignOptionType::Text,
        label: "Note".into(),
        default: json!(""),
        choices: None,
        max_length: Some(5),
    };
    assert!(validate_option_value(&text, &json!("hello")).is_ok());
    assert!(validate_option_value(&text, &json!("hello!")).is_err());
    assert!(validate_option_value(&text, &json!(7)).is_err());
}
