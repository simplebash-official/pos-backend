use super::sync_mongo_codec::camel_to_snake;
use super::sync_mongo_codec::encode;
use super::sync_mongo_codec::plain_derived;

use mongodb::bson::Bson;

use crate::modules::sync::resources::spec;
use serde_json::json;

#[test]
fn snake_case_conversion() {
    assert_eq!(camel_to_snake("costPriceCents"), "cost_price_cents");
    assert_eq!(camel_to_snake("cardLast4"), "card_last4");
    assert_eq!(camel_to_snake("key"), "key");
    assert_eq!(camel_to_snake("sourceTicketKey"), "source_ticket_key");
}

#[test]
fn product_payload_is_split_into_fields_and_derived() {
    let product = spec("products").unwrap();
    let encoded = encode(
        product,
        &json!({
            "id": "65a1b2c3d4e5f60718293a4b", "key": "prd_1", "name": "Screen",
            "category": "Phones", "subcategory": "Screens", "categoryKey": "cat_1",
            "stockQuantity": 7, "sellingPriceCents": 1000, "version": 3,
            "createdAt": "2026-05-01T10:00:00.000Z", "updatedAt": "2026-05-01T11:00:00.000Z",
            "deletedAt": null, "updatedByDevice": "dev_x"
        }),
    )
    .unwrap();
    assert_eq!(encoded.id.to_hex(), "65a1b2c3d4e5f60718293a4b");
    assert_eq!(encoded.fields.get_str("name").unwrap(), "Screen");
    assert_eq!(encoded.fields.get_str("category_key").unwrap(), "cat_1");
    // Read-time enrichments and merge-managed columns are not stored fields.
    for absent in [
        "category",
        "subcategory",
        "updated_at",
        "version",
        "id",
        "updated_by_device",
    ] {
        assert!(encoded.fields.get(absent).is_none(), "{absent}");
    }
    assert_eq!(encoded.derived.get_i64("stock_quantity").unwrap(), 7);
    assert_eq!(encoded.version, 3);
    assert!(encoded.created_at.is_some());
    assert!(encoded.deleted_at.is_none());
}

#[test]
fn stock_movement_product_id_becomes_an_object_id() {
    let movement = spec("stockMovements").unwrap();
    let encoded = encode(
        movement,
        &json!({ "id": "65a1b2c3d4e5f60718293a4b", "key": "sm_1",
                     "productId": "65a1b2c3d4e5f60718293a4c", "quantityDelta": -2 }),
    )
    .unwrap();
    assert!(matches!(
        encoded.fields.get("product_id"),
        Some(Bson::ObjectId(_))
    ));
}

#[test]
fn payloads_with_operator_keys_or_bad_ids_are_rejected() {
    let product = spec("products").unwrap();
    let ok_id = "65a1b2c3d4e5f60718293a4b";
    assert!(encode(product, &json!({ "id": ok_id, "meta": { "$set": 1 } })).is_err());
    assert!(encode(product, &json!({ "id": "nope" })).is_err());
    assert!(encode(product, &json!({ "id": ok_id, "createdAt": "yesterday" })).is_err());
    assert!(encode(product, &json!([1, 2])).is_err());
}

#[test]
fn payload_without_id_gets_a_fresh_one_and_lifecycle_status_is_not_derived() {
    let category = spec("categories").unwrap();
    let encoded = encode(category, &json!({ "key": "cat_1", "name": "Phones" })).unwrap();
    assert_eq!(encoded.fields.get_str("key").unwrap(), "cat_1");

    let invoice = spec("invoices").unwrap();
    let derived = plain_derived(invoice);
    assert!(derived.contains(&"refunded_cents"));
    assert!(!derived.contains(&"status"));
    assert!(derived.iter().all(|c| !c.contains('[')));
}

#[test]
fn null_fields_are_omitted_not_stored() {
    let customer = spec("customers").unwrap();
    let encoded = encode(
        customer,
        &json!({ "id": "65a1b2c3d4e5f60718293a4b", "key": "cus_1", "email": null }),
    )
    .unwrap();
    assert!(encoded.fields.get("email").is_none());
}
