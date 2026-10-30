use super::origin_field::ORIGIN_FIELD;
use super::origin_for::origin_for;
use super::stamp_update::stamp_update;
use mongodb::bson::doc;
use mongodb::options::UpdateModifications;

#[test]
fn origin_prefers_the_device_then_the_browser_then_cloud() {
    assert_eq!(origin_for(Some("dev_1"), Some("abc")), "dev_1");
    assert_eq!(origin_for(None, Some("abc")), "web:abc");
    assert_eq!(origin_for(Some(" "), Some(" ")), "cloud");
    assert_eq!(origin_for(None, None), "cloud");
}

#[test]
fn operator_updates_get_the_origin_in_set() {
    let out = stamp_update(
        "web:x",
        UpdateModifications::Document(doc! { "$set": { "name": "a", ORIGIN_FIELD: "dev_old" } }),
    );
    let UpdateModifications::Document(d) = out else {
        panic!()
    };
    assert_eq!(
        d.get_document("$set")
            .unwrap()
            .get_str(ORIGIN_FIELD)
            .unwrap(),
        "web:x"
    );
    assert_eq!(
        d.get_document("$set").unwrap().get_str("name").unwrap(),
        "a"
    );
}

#[test]
fn updates_without_set_gain_one_and_conflicts_are_removed() {
    let out = stamp_update(
        "cloud",
        UpdateModifications::Document(doc! {
            "$inc": { "version": 1 },
            "$setOnInsert": { ORIGIN_FIELD: "x" },
        }),
    );
    let UpdateModifications::Document(d) = out else {
        panic!()
    };
    assert_eq!(
        d.get_document("$set")
            .unwrap()
            .get_str(ORIGIN_FIELD)
            .unwrap(),
        "cloud"
    );
    assert!(d.get("$setOnInsert").is_none());
    assert!(d.get("$inc").is_some());
}

#[test]
fn pipeline_updates_get_a_trailing_set_stage() {
    let out = stamp_update(
        "dev_1",
        UpdateModifications::Pipeline(vec![doc! { "$set": { "a": 1 } }]),
    );
    let UpdateModifications::Pipeline(stages) = out else {
        panic!()
    };
    assert_eq!(stages.len(), 2);
    assert_eq!(stages[1], doc! { "$set": { ORIGIN_FIELD: "dev_1" } });
}
